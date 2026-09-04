use std::ffi::OsString;

use xccute_decisions::{
    DecisionGuideTemplate,
    DecisionPathStep,
    DecisionPathTemplate,
    DecisionQuestionSpec,
    DecisionRunbookJournal,
    DecisionRunbookJournalEntry,
    DecisionRunbookJournalError,
    DecisionRunbookRecord,
    DecisionRunbookTemplate,
    grep_observation_tool,
};
use xccute_runtime::*;

fn op(logical_id: &str, program: &str, argv: &[&str]) -> RuntimeOperation {
    RuntimeOperation::new(
        logical_id,
        OsString::from(program),
        argv.iter().map(|arg| OsString::from(*arg)).collect(),
        format!("{} {}", program, argv.join(" ")),
    )
}

fn plan() -> RuntimeOperationPlan {
    RuntimeOperationPlan::new("nodeplan.bootstrap")
        .then(op("logs.grep_errors", "grep", &["ERROR", "nodeplan.log"]))
        .then(op("nodeplan.apply_dry_run", "nodeplanctl", &["apply", "--dry-run"]))
        .then(op("nodeplan.commit_receipt", "nodeplanctl", &["receipt", "commit"]))
}

fn material_contract(plan: &RuntimeOperationPlan) -> (RuntimeMaterialManifest, RuntimePlanMaterialContract) {
    let manifest = RuntimeMaterialManifest::new("nodeplan.materials");
    let contract = RuntimePlanMaterialContract::new(plan, &manifest);
    (manifest, contract)
}

fn guide() -> DecisionGuideTemplate {
    let path = DecisionPathTemplate::new(
        "nodeplan.error_review.path",
        "Ask one bounded question before dry-run apply.",
    )
    .step(DecisionPathStep::required_observation(
        "ask.error_pattern",
        "grep.pattern_search",
        "check.errors",
        "logs.grep_errors",
        "Need focused error evidence before deciding.",
    ))
    .step(DecisionPathStep::operation(
        "apply.dry_run",
        "nodeplan.apply_dry_run",
        "Dry-run can happen after required evidence exists.",
    ));

    DecisionGuideTemplate::new(
        "nodeplan.error_review.guide",
        "Decide whether the NodePlan dry-run should run.",
        path,
    )
    .ask(DecisionQuestionSpec::required(
        "check.errors",
        "logs.grep_errors",
        &grep_observation_tool(),
        "Did grep find blocking ERROR lines?",
    ))
}

fn observation_runbook(runbook_id: &str) -> DecisionRunbookTemplate {
    DecisionRunbookTemplate::observation_only(
        runbook_id,
        "Only ask focused questions and acknowledge the decision path.",
        guide(),
    )
}

fn observation_context(
    plan: &RuntimeOperationPlan,
    manifest: &RuntimeMaterialManifest,
    material_contract: &RuntimePlanMaterialContract,
    instance: &xccute_decisions::DecisionRunbookInstance,
) -> RuntimeGuidedDecisionContext {
    let grep_operation = plan
        .operation_by_logical_id("logs.grep_errors")
        .expect("grep operation exists");
    let call = RuntimeConnectorCall::for_operation(
        RuntimeConnectorIdentity::new("nodeplan.local", "nodeplan"),
        "nodeplan-control",
        "grep_errors",
        plan,
        grep_operation,
        "collect bounded error evidence",
    )
    .expect("grep call should form");
    let intent = RuntimeConnectorExecutionIntent::prepare(call, material_contract, manifest)
        .expect("empty material manifest should satisfy gate");
    let requirement = instance.runtime_guide.questions[0].to_observation_requirement();
    let observation_call = RuntimeObservationCall::for_intent(
        &requirement,
        &intent,
        "check whether errors block dry-run apply",
    )
    .expect("observation call should form");
    let fact = RuntimeObservationFact::from_text(
        &observation_call,
        "fact.error_pattern",
        false,
        0,
        "",
        "grep found 0 ERROR lines",
        "no blocking error evidence, so continue",
    );
    let evidence = RuntimeObservationEvidenceSet::new(plan).with_fact(fact);

    RuntimeGuidedDecisionContext::from_evidence(
        &instance.runtime_guide,
        &instance.observation_plan,
        &evidence,
    )
    .expect("required evidence should form guided context")
}

fn observation_record(runbook_id: &str, acknowledged_reason: &str, recorded_reason: &str) -> DecisionRunbookRecord {
    let plan = plan();
    let (manifest, material_contract) = material_contract(&plan);
    let instance = observation_runbook(runbook_id)
        .materialize(&plan, &manifest, &material_contract, "nodeplan.observations")
        .expect("observation-only runbook should materialize");
    let context = observation_context(&plan, &manifest, &material_contract, &instance);
    let transition = plan
        .transition_after(
            "logs.grep_errors",
            &ExitStatusDecision {
                status: RuntimeExitStatus::new(Some(0), true),
                disposition: ExitDisposition::Continue,
                reason: "observation found no blocking evidence".to_string(),
            },
        )
        .expect("observation transition should form");
    let acknowledged = RuntimeAcknowledgedDecisionPath::new(
        &context,
        &transition,
        acknowledged_reason,
    );

    DecisionRunbookRecord::observation_only(&instance, &context, &acknowledged, recorded_reason)
        .expect("observation-only record should form")
}

#[test]
fn journal_appends_records_into_ordered_digest_chain() {
    let first = observation_record(
        "nodeplan.error_review.runbook",
        "operator accepted grep evidence",
        "grep evidence showed no blocking errors",
    );
    let second = observation_record(
        "nodeplan.process_review.runbook",
        "operator accepted process evidence",
        "process evidence allowed next step",
    );

    let journal = DecisionRunbookJournal::new("nodeplan.bootstrap.journal")
        .append(&first, "grep found no blocking ERROR lines")
        .append(&second, "pgrep/process review allowed dry-run path");

    assert_eq!(journal.entries.len(), 2);
    assert_eq!(journal.entries[0].previous_entry_digest, None);
    assert_eq!(journal.entries[1].previous_entry_digest, Some(journal.entries[0].digest()));
    assert_eq!(journal.digest(), journal.digest());

    let replay = journal.replay_steps();
    assert_eq!(replay.len(), 2);
    assert_eq!(replay[0].compact_summary, "grep found no blocking ERROR lines");
    assert!(journal.compact_context().contains("pgrep/process review allowed"));

    let validated = DecisionRunbookJournal::try_from_entries(
        "nodeplan.bootstrap.journal",
        journal.entries.clone(),
    )
    .expect("valid append-only entries should validate");
    assert_eq!(validated.digest(), journal.digest());
}

#[test]
fn journal_rejects_broken_previous_entry_link() {
    let first = observation_record("nodeplan.error_review.runbook", "ack one", "record one");
    let second = observation_record("nodeplan.process_review.runbook", "ack two", "record two");
    let journal = DecisionRunbookJournal::new("nodeplan.bootstrap.journal")
        .append(&first, "first summary")
        .append(&second, "second summary");
    let mut entries = journal.entries.clone();
    entries[1].previous_entry_digest = Some(StableDigest::sha256("wrong previous digest"));

    let result = DecisionRunbookJournal::try_from_entries("nodeplan.bootstrap.journal", entries);

    assert!(matches!(
        result,
        Err(DecisionRunbookJournalError::PreviousEntryDigestMismatch { index: 1, .. })
    ));
}

#[test]
fn journal_rejects_out_of_order_index() {
    let first = observation_record("nodeplan.error_review.runbook", "ack one", "record one");
    let journal = DecisionRunbookJournal::new("nodeplan.bootstrap.journal")
        .append(&first, "first summary");
    let mut entries = journal.entries.clone();
    entries[0].index = 7;

    let result = DecisionRunbookJournal::try_from_entries("nodeplan.bootstrap.journal", entries);

    assert!(matches!(
        result,
        Err(DecisionRunbookJournalError::EntryIndexMismatch {
            expected_index: 0,
            actual_index: 7,
        })
    ));
}

#[test]
fn journal_rejects_duplicate_record_digest() {
    let record = observation_record("nodeplan.error_review.runbook", "ack one", "record one");
    let first = DecisionRunbookJournalEntry::new(0, None, &record, "first summary");
    let second = DecisionRunbookJournalEntry::new(1, Some(first.digest()), &record, "second summary");

    let result = DecisionRunbookJournal::try_from_entries(
        "nodeplan.bootstrap.journal",
        vec![first, second],
    );

    assert!(matches!(
        result,
        Err(DecisionRunbookJournalError::DuplicateRecordDigest {
            first_index: 0,
            duplicate_index: 1,
            ..
        })
    ));
}

#[test]
fn journal_digest_tracks_order_and_compact_summary() {
    let first = observation_record("nodeplan.error_review.runbook", "ack one", "record one");
    let second = observation_record("nodeplan.process_review.runbook", "ack two", "record two");

    let journal_a = DecisionRunbookJournal::new("nodeplan.bootstrap.journal")
        .append(&first, "first summary")
        .append(&second, "second summary");
    let journal_b = DecisionRunbookJournal::new("nodeplan.bootstrap.journal")
        .append(&second, "second summary")
        .append(&first, "first summary");
    let journal_c = DecisionRunbookJournal::new("nodeplan.bootstrap.journal")
        .append(&first, "changed first summary")
        .append(&second, "second summary");

    assert_ne!(journal_a.digest(), journal_b.digest());
    assert_ne!(journal_a.digest(), journal_c.digest());
}
