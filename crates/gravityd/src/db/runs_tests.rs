use bus::{now, OverlapPolicy, RoutineRunState, RunSource, Trigger};

use super::{tests::setup, NewRun};

#[test]
fn routine_occurrence_unique() {
    let (db, bot) = setup();
    let routine = db
        .create_routine(
            &bot.id,
            "daily",
            &Trigger::Interval { seconds: 60 },
            "/report",
            OverlapPolicy::Skip,
            true,
        )
        .unwrap();
    let scheduled_for = now();
    let occurrence = |scheduled_for| NewRun {
        routine_id: &routine.id,
        scheduled_for,
        source: RunSource::Schedule,
        signal_id: None,
    };

    assert!(db
        .schedule_routine_run(occurrence(scheduled_for))
        .unwrap()
        .is_some());
    assert!(db
        .schedule_routine_run(occurrence(scheduled_for))
        .unwrap()
        .is_none());
}

#[test]
fn routine_retry_gets_a_fresh_message_and_delivery() {
    let (db, bot) = setup();
    let routine = db
        .create_routine(
            &bot.id,
            "daily",
            &Trigger::Interval { seconds: 60 },
            "/report",
            OverlapPolicy::Skip,
            true,
        )
        .unwrap();
    db.schedule_routine_run(NewRun {
        routine_id: &routine.id,
        scheduled_for: now(),
        source: RunSource::Manual,
        signal_id: None,
    })
    .unwrap();
    let first_run = db.lease_due_routine_runs(60).unwrap().remove(0);
    let first = db
        .dispatch_routine_run(&routine, &first_run, &routine.prompt)
        .unwrap()
        .unwrap();
    db.mark_delivered(&first.delivery.id).unwrap();

    assert!(db
        .retry_routine_run(&first_run.id, now(), "deadline exceeded")
        .unwrap());
    let second_run = db.lease_due_routine_runs(60).unwrap().remove(0);
    assert_eq!(second_run.attempt, 2);
    let second = db
        .dispatch_routine_run(&routine, &second_run, &routine.prompt)
        .unwrap()
        .unwrap();

    assert_ne!(first.message.id, second.message.id);
    assert_ne!(first.delivery.id, second.delivery.id);
}

#[test]
fn cancelled_run_cannot_dispatch_after_the_state_transition() {
    let (db, bot) = setup();
    let routine = db
        .create_routine(
            &bot.id,
            "daily",
            &Trigger::Interval { seconds: 60 },
            "/report",
            OverlapPolicy::Skip,
            true,
        )
        .unwrap();
    db.schedule_routine_run(NewRun {
        routine_id: &routine.id,
        scheduled_for: now(),
        source: RunSource::Manual,
        signal_id: None,
    })
    .unwrap();
    let run = db.lease_due_routine_runs(60).unwrap().remove(0);

    assert!(db.cancel_routine_run(&run.id, "cancelled").unwrap());
    assert!(db
        .dispatch_routine_run(&routine, &run, &routine.prompt)
        .unwrap()
        .is_none());
    assert!(db.list_deliveries(Some(&bot.id), None).unwrap().is_empty());
}

#[test]
fn overlap_transitions_cannot_revive_or_overwrite_a_cancelled_run() {
    let (db, bot) = setup();
    let routine = db
        .create_routine(
            &bot.id,
            "daily",
            &Trigger::Interval { seconds: 60 },
            "/report",
            OverlapPolicy::QueueOne,
            true,
        )
        .unwrap();
    let scheduled = db
        .schedule_routine_run(NewRun {
            routine_id: &routine.id,
            scheduled_for: now(),
            source: RunSource::Manual,
            signal_id: None,
        })
        .unwrap()
        .unwrap();
    assert!(db.cancel_routine_run(&scheduled.id, "cancelled").unwrap());
    assert!(!db
        .finish_scheduled_routine_run(&scheduled.id, RoutineRunState::Skipped, Some("collapsed"))
        .unwrap());

    let second = db
        .schedule_routine_run(NewRun {
            routine_id: &routine.id,
            scheduled_for: now() - chrono::Duration::seconds(1),
            source: RunSource::Manual,
            signal_id: None,
        })
        .unwrap()
        .unwrap();
    let running = db.lease_due_routine_runs(60).unwrap().remove(0);
    assert_eq!(running.id, second.id);
    assert!(db.cancel_routine_run(&running.id, "cancelled").unwrap());
    assert!(!db.defer_routine_run(&running.id, "waiting").unwrap());

    for run_id in [scheduled.id, running.id] {
        assert_eq!(
            db.get_routine_run(&run_id).unwrap().unwrap().state,
            RoutineRunState::Cancelled
        );
    }
}

#[test]
fn routine_completion_updates_only_the_correlated_run() {
    let (db, bot) = setup();
    let routine = db
        .create_routine(
            &bot.id,
            "daily",
            &Trigger::Interval { seconds: 60 },
            "/report",
            OverlapPolicy::QueueAll,
            true,
        )
        .unwrap();
    for offset in [0, 1] {
        db.schedule_routine_run(NewRun {
            routine_id: &routine.id,
            scheduled_for: now() - chrono::Duration::seconds(offset),
            source: RunSource::Manual,
            signal_id: None,
        })
        .unwrap();
    }
    let runs = db.lease_due_routine_runs(60).unwrap();
    assert_eq!(runs.len(), 2);

    assert!(db
        .complete_routine_run_for_bot(&runs[0].id, &bot.id)
        .unwrap());
    assert_eq!(
        db.get_routine_run(&runs[0].id).unwrap().unwrap().state,
        RoutineRunState::Succeeded
    );
    assert_eq!(
        db.get_routine_run(&runs[1].id).unwrap().unwrap().state,
        RoutineRunState::Running
    );
}
