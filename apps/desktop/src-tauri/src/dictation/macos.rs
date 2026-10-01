//! Dictation on macOS: `AVAudioEngine` feeds the microphone to an
//! `SFSpeechAudioBufferRecognitionRequest`, recognized on the device.

use std::ptr::NonNull;
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Duration;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2_avf_audio::{AVAudioEngine, AVAudioPCMBuffer, AVAudioTime};
use objc2_foundation::NSError;
use objc2_speech::{
    SFSpeechAudioBufferRecognitionRequest, SFSpeechRecognitionResult, SFSpeechRecognitionTask,
    SFSpeechRecognizer, SFSpeechRecognizerAuthorizationStatus,
};
use tauri::AppHandle;

use super::{emit, DictationEvent};

/// Samples per microphone buffer handed to the recognizer.
const TAP_FRAMES: u32 = 1024;

/// A running session's objects. They are only touched under `SESSION`'s
/// lock, and AVAudioEngine and the Speech objects accept calls from any
/// thread, so moving them between threads is sound.
struct Session {
    engine: Retained<AVAudioEngine>,
    request: Retained<SFSpeechAudioBufferRecognitionRequest>,
    task: Retained<SFSpeechRecognitionTask>,
    _recognizer: Retained<SFSpeechRecognizer>,
}

// SAFETY: see `Session`; the objects are used from one thread at a time.
unsafe impl Send for Session {}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);
/// A stopped session still finishing its transcript; kept alive until the
/// next one starts, so its final result is not cut off.
static FINISHING: Mutex<Option<Session>> = Mutex::new(None);

fn lock(slot: &'static Mutex<Option<Session>>) -> std::sync::MutexGuard<'static, Option<Session>> {
    slot.lock().unwrap_or_else(|e| e.into_inner())
}

/// The speech permission, asking the first time.
fn authorize() -> Result<(), String> {
    let status = unsafe { SFSpeechRecognizer::authorizationStatus() };
    let status = if status == SFSpeechRecognizerAuthorizationStatus::NotDetermined {
        let (tx, rx) = mpsc::channel();
        let handler = RcBlock::new(move |answer: SFSpeechRecognizerAuthorizationStatus| {
            let _ = tx.send(answer);
        });
        unsafe { SFSpeechRecognizer::requestAuthorization(&handler) };
        rx.recv_timeout(Duration::from_secs(120))
            .map_err(|_| "No answer to the speech recognition permission prompt.".to_string())?
    } else {
        status
    };
    if status == SFSpeechRecognizerAuthorizationStatus::Authorized {
        Ok(())
    } else {
        Err("Speech recognition is off for Gravity. Turn it on in System Settings › Privacy & Security › Speech Recognition.".to_string())
    }
}

pub(super) fn start(app: AppHandle) -> Result<(), String> {
    stop();
    authorize()?;
    let recognizer = unsafe { SFSpeechRecognizer::new() };
    if !unsafe { recognizer.isAvailable() } {
        return Err("Speech recognition is not available right now.".to_string());
    }
    let request = unsafe { SFSpeechAudioBufferRecognitionRequest::new() };
    unsafe {
        request.setShouldReportPartialResults(true);
        request.setAddsPunctuation(true);
        // Keep the audio on this Mac whenever it can recognize locally.
        if recognizer.supportsOnDeviceRecognition() {
            request.setRequiresOnDeviceRecognition(true);
        }
    }

    let engine = unsafe { AVAudioEngine::new() };
    let input = unsafe { engine.inputNode() };
    let format = unsafe { input.outputFormatForBus(0) };
    let feed = request.clone();
    let tap = RcBlock::new(
        move |buffer: NonNull<AVAudioPCMBuffer>, _: NonNull<AVAudioTime>| {
            unsafe { feed.appendAudioPCMBuffer(buffer.as_ref()) };
        },
    );
    unsafe {
        input.installTapOnBus_bufferSize_format_block(
            0,
            TAP_FRAMES,
            Some(&format),
            RcBlock::as_ptr(&tap),
        );
        engine.prepare();
    }
    if let Err(error) = unsafe { engine.startAndReturnError() } {
        unsafe { input.removeTapOnBus(0) };
        return Err(format!(
            "The microphone could not start ({}). Check System Settings › Privacy & Security › Microphone.",
            error.localizedDescription()
        ));
    }

    let results = app.clone();
    let handler = RcBlock::new(
        move |result: *mut SFSpeechRecognitionResult, error: *mut NSError| {
            if let Some(result) = unsafe { result.as_ref() } {
                let text = unsafe { result.bestTranscription().formattedString() }.to_string();
                if unsafe { result.isFinal() } {
                    emit(&results, DictationEvent::Final { text });
                    return;
                }
                emit(&results, DictationEvent::Partial { text });
            }
            if let Some(error) = unsafe { error.as_ref() } {
                emit(
                    &results,
                    DictationEvent::Ended {
                        error: Some(error.localizedDescription().to_string()),
                    },
                );
            }
        },
    );
    let task = unsafe { recognizer.recognitionTaskWithRequest_resultHandler(&request, &handler) };
    *lock(&SESSION) = Some(Session {
        engine,
        request,
        task,
        _recognizer: recognizer,
    });
    Ok(())
}

/// Stops the microphone and lets the recognizer settle the transcript.
pub(super) fn stop() {
    let Some(session) = lock(&SESSION).take() else {
        return;
    };
    unsafe {
        session.engine.stop();
        session.engine.inputNode().removeTapOnBus(0);
        session.request.endAudio();
        session.task.finish();
    }
    *lock(&FINISHING) = Some(session);
}
