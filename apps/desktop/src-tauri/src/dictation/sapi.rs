//! Dictation on Windows: the SAPI in-process recognizer with its dictation
//! grammar, fed by the default microphone. SAPI's desktop recognizers run
//! entirely on this PC, unlike the WinRT recognizer, whose dictation needs
//! Windows' online speech service.
//!
//! Each session owns a thread that holds the COM objects, so nothing SAPI
//! hands out crosses threads. Phrases SAPI settles are kept; the phrase being
//! spoken shows as a hypothesis after them.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::AppHandle;
use windows::core::{IUnknown, Interface, PWSTR};
use windows::Win32::Media::Speech::{
    ISpObjectToken, ISpObjectTokenCategory, ISpRecoContext, ISpRecoGrammar, ISpRecoResult,
    ISpRecognizer, SpInprocRecognizer, SpMMAudioIn, SpObjectToken, SpObjectTokenCategory,
    SPCAT_RECOGNIZERS, SPEI_END_SR_STREAM, SPEI_FALSE_RECOGNITION, SPEI_HYPOTHESIS,
    SPEI_RECOGNITION, SPEI_RESERVED1, SPEI_RESERVED2, SPET_LPARAM_IS_OBJECT,
    SPET_LPARAM_IS_POINTER, SPET_LPARAM_IS_STRING, SPET_LPARAM_IS_TOKEN, SPEVENT, SPEVENTENUM,
    SPLO_STATIC, SPRST_ACTIVE, SPRST_INACTIVE, SPRS_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED,
};

use super::{emit, DictationEvent};

/// How often the session thread looks for a stop request between events.
const POLL: Duration = Duration::from_millis(100);
/// After the microphone stops, how long to wait for the last phrase to
/// settle, and the most to wait in all. The composer stops listening to
/// events three seconds after it asks to stop.
const SETTLE_QUIET: Duration = Duration::from_millis(600);
const SETTLE_MAX: Duration = Duration::from_millis(1500);
/// `SP_GETWHOLEPHRASE` from sapi.h: the start and count that cover a phrase.
const WHOLE_PHRASE: u32 = u32::MAX;

/// Asks the running session's thread to stop.
static STOP: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

fn stop_slot() -> std::sync::MutexGuard<'static, Option<Arc<AtomicBool>>> {
    STOP.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether a SAPI recognizer is installed (Windows ships one per supported
/// display language).
pub(super) fn available() -> bool {
    std::thread::spawn(|| {
        with_com(|| unsafe {
            default_recognizer()?;
            Ok(())
        })
        .is_ok()
    })
    .join()
    .unwrap_or(false)
}

pub(super) fn start(app: AppHandle) -> Result<(), String> {
    stop();
    let stopping = Arc::new(AtomicBool::new(false));
    let (ready, started) = mpsc::channel();
    let flag = stopping.clone();
    std::thread::Builder::new()
        .name("dictation".into())
        .spawn(move || {
            let _ = with_com(|| {
                let session = match unsafe { Session::open() } {
                    Ok(session) => session,
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return Ok(());
                    }
                };
                let _ = ready.send(Ok(()));
                session.run(&app, &flag);
                Ok(())
            });
        })
        .map_err(|e| e.to_string())?;
    started
        .recv()
        .map_err(|_| "Dictation stopped before it started.".to_string())??;
    *stop_slot() = Some(stopping);
    Ok(())
}

/// Stops the microphone; the session settles its transcript on its thread.
pub(super) fn stop() {
    if let Some(stopping) = stop_slot().take() {
        stopping.store(true, Ordering::SeqCst);
    }
}

/// Runs `work` with COM initialized on this thread.
fn with_com<T>(work: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
        .ok()
        .map_err(|e| e.message())?;
    let result = work();
    unsafe { CoUninitialize() };
    result
}

/// The installed recognizer the user picked in Speech settings.
unsafe fn default_recognizer() -> Result<ISpObjectToken, String> {
    let category: ISpObjectTokenCategory =
        CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_ALL).map_err(|e| e.message())?;
    category
        .SetId(SPCAT_RECOGNIZERS, false)
        .map_err(|e| e.message())?;
    let id = category
        .GetDefaultTokenId()
        .map_err(|_| "No speech recognizer is installed for this language.".to_string())?;
    let token: Result<ISpObjectToken, String> =
        CoCreateInstance(&SpObjectToken, None, CLSCTX_ALL).map_err(|e| e.message());
    let token = token.and_then(|token| {
        token
            .SetId(None, windows::core::PCWSTR(id.0), false)
            .map_err(|e| e.message())?;
        Ok(token)
    });
    CoTaskMemFree(Some(id.0 as _));
    token
}

/// The bit `SetInterest` takes for one event; sapi.h's `SPFEI`.
fn interest(event: SPEVENTENUM) -> u64 {
    (1u64 << event.0) | (1u64 << SPEI_RESERVED1.0) | (1u64 << SPEI_RESERVED2.0)
}

struct Session {
    recognizer: ISpRecognizer,
    context: ISpRecoContext,
    _grammar: ISpRecoGrammar,
}

/// What has been said so far.
#[derive(Default)]
struct Transcript {
    settled: Vec<String>,
    speaking: Option<String>,
}

impl Transcript {
    fn text(&self) -> String {
        let mut parts: Vec<&str> = self.settled.iter().map(String::as_str).collect();
        if let Some(speaking) = &self.speaking {
            parts.push(speaking);
        }
        parts.join(" ")
    }
}

impl Session {
    unsafe fn open() -> Result<Self, String> {
        let recognizer: ISpRecognizer =
            CoCreateInstance(&SpInprocRecognizer, None, CLSCTX_ALL).map_err(|e| e.message())?;
        recognizer
            .SetRecognizer(&default_recognizer()?)
            .map_err(|e| e.message())?;
        let microphone: IUnknown =
            CoCreateInstance(&SpMMAudioIn, None, CLSCTX_ALL).map_err(|e| e.message())?;
        recognizer
            .SetInput(&microphone, true)
            .map_err(|_| microphone_off())?;
        let context = recognizer.CreateRecoContext().map_err(|e| e.message())?;
        context.SetNotifyWin32Event().map_err(|e| e.message())?;
        let events = [
            SPEI_RECOGNITION,
            SPEI_HYPOTHESIS,
            SPEI_FALSE_RECOGNITION,
            SPEI_END_SR_STREAM,
        ]
        .into_iter()
        .fold(0, |mask, event| mask | interest(event));
        context
            .SetInterest(events, events)
            .map_err(|e| e.message())?;
        let grammar = context.CreateGrammar(0).map_err(|e| e.message())?;
        grammar
            .LoadDictation(None, SPLO_STATIC)
            .map_err(|e| e.message())?;
        grammar
            .SetDictationState(SPRS_ACTIVE)
            .map_err(|e| e.message())?;
        recognizer
            .SetRecoState(SPRST_ACTIVE)
            .map_err(|_| microphone_off())?;
        Ok(Self {
            recognizer,
            context,
            _grammar: grammar,
        })
    }

    /// Streams the transcript until asked to stop, then settles it.
    fn run(self, app: &AppHandle, stopping: &AtomicBool) {
        let mut transcript = Transcript::default();
        while !stopping.load(Ordering::SeqCst) {
            unsafe {
                let _ = self.context.WaitForNotifyEvent(POLL.as_millis() as u32);
            }
            match self.drain(&mut transcript) {
                Ok(true) => emit(
                    app,
                    DictationEvent::Partial {
                        text: transcript.text(),
                    },
                ),
                Ok(false) => {}
                Err(error) => {
                    emit(app, DictationEvent::Ended { error: Some(error) });
                    return;
                }
            }
        }

        // The microphone stops; the phrase in progress may still settle.
        unsafe {
            let _ = self.recognizer.SetRecoState(SPRST_INACTIVE);
        }
        let began = Instant::now();
        let mut quiet_since = Instant::now();
        while began.elapsed() < SETTLE_MAX && quiet_since.elapsed() < SETTLE_QUIET {
            unsafe {
                let _ = self.context.WaitForNotifyEvent(POLL.as_millis() as u32);
            }
            if let Ok(true) = self.drain(&mut transcript) {
                quiet_since = Instant::now();
            }
        }
        emit(
            app,
            DictationEvent::Final {
                text: transcript.text(),
            },
        );
    }

    /// Takes the queued events into `transcript`; true when it changed.
    fn drain(&self, transcript: &mut Transcript) -> Result<bool, String> {
        let mut changed = false;
        loop {
            let mut events = [SPEVENT::default(); 16];
            let mut fetched = 0u32;
            unsafe {
                self.context
                    .GetEvents(events.len() as u32, events.as_mut_ptr(), &mut fetched)
                    .map_err(|e| e.message())?;
            }
            if fetched == 0 {
                return Ok(changed);
            }
            for event in &events[..fetched as usize] {
                let id = SPEVENTENUM(event._bitfield & 0xffff);
                if id == SPEI_END_SR_STREAM {
                    // lParam carries the audio stream's HRESULT.
                    let status = windows::core::HRESULT(event.lParam.0 as i32);
                    if status.is_err() {
                        return Err(microphone_off());
                    }
                    continue;
                }
                let text = unsafe { take_result_text(event) };
                match id {
                    SPEI_RECOGNITION => {
                        transcript.speaking = None;
                        if let Some(text) = text.filter(|t| !t.trim().is_empty()) {
                            transcript.settled.push(text.trim().to_string());
                        }
                        changed = true;
                    }
                    SPEI_HYPOTHESIS => {
                        transcript.speaking = text.map(|t| t.trim().to_string());
                        changed = true;
                    }
                    SPEI_FALSE_RECOGNITION => {
                        changed |= transcript.speaking.take().is_some();
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Reads a recognition event's phrase and releases what the event holds
/// (sapi.h's `SpClearEvent`).
unsafe fn take_result_text(event: &SPEVENT) -> Option<String> {
    let param = event.lParam.0 as *mut core::ffi::c_void;
    if param.is_null() {
        return None;
    }
    let kind = (event._bitfield >> 16) & 0xffff;
    if kind == SPET_LPARAM_IS_OBJECT.0 || kind == SPET_LPARAM_IS_TOKEN.0 {
        let object = IUnknown::from_raw(param);
        let result: ISpRecoResult = object.cast().ok()?;
        let mut text = PWSTR::null();
        result
            .GetText(WHOLE_PHRASE, WHOLE_PHRASE, true, &mut text, None)
            .ok()?;
        let phrase = text.to_string().ok();
        CoTaskMemFree(Some(text.0 as _));
        return phrase;
    }
    if kind == SPET_LPARAM_IS_POINTER.0 || kind == SPET_LPARAM_IS_STRING.0 {
        CoTaskMemFree(Some(param as _));
    }
    None
}

fn microphone_off() -> String {
    "The microphone could not start. Check Settings › Privacy & security › Microphone, \
     including \"Let desktop apps access your microphone\"."
        .to_string()
}
