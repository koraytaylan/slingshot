use super::*;

#[test]
fn capacity_pauses_immediately_but_eligibility_alone_does_not_pause_other_retries() {
    let mut fact = RecoveryFact {
        attempt_count: 0,
        category: RecoveryCategory::PersistentCapacityUnavailable,
        detail: "capacity unavailable".to_owned(),
        evidence: RecoveryExecutionEvidence::AuthoritativeRemoteSuccess,
        manual_resume_eligible: true,
        retry_delay_milliseconds: 0,
        retry_observed_at_unix_milliseconds: 1,
    };
    assert!(automatic_recovery_paused(&fact));
    fact.category = RecoveryCategory::ResultAcquisition;
    assert!(!automatic_recovery_paused(&fact));
    fact.attempt_count =
        u32::try_from(crate::operation::recovery_and_event_supervisor::automatic_attempt_cap())
            .unwrap();
    assert!(automatic_recovery_paused(&fact));
    fact.manual_resume_eligible = false;
    assert!(!automatic_recovery_paused(&fact));
}
