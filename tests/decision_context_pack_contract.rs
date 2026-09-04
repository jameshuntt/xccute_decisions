use std::ffi::OsString;

use xccute_decisions::{
    DecisionContextPack,
    DecisionContextPackError,
    DecisionContextPackReplaySelection,
    DecisionGuideTemplate,
    DecisionPathStep,
    DecisionPathTemplate,
    DecisionQuestionSpec,
    DecisionRunbookJournal,
    DecisionRunbookRecord,
    DecisionRunbookTemplate,
    grep_observation_tool,
    pgrep_observation_tool,
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
        .then(op("proc.find_nodeplan", "pgrep", &["nodeplanctl"]))
        .then(op("nodeplan.apply_dry_run", "nodeplanctl", &["apply", "--dry-run"]))
}

fn material_contract(plan: &RuntimeOperationPlan) -> (RuntimeMaterialManifest, RuntimePlanMaterialContract) {
    let manifest = RuntimeMaterialManifest::new("nodeplan.materials");
    let contract = RuntimePlanMaterialContract::new(plan, &manifest);
    (manifest, contract)
}

fn guide() -> DecisionGuideTemplate {
    let path = DecisionPathTemplate::new(
        "nodeplan.error_review.path",
        "Ask bounded questions before dry-run apply.",
    )
    .step(DecisionPathStep::required_observation(
        "ask.error_pattern",
        "grep.pattern_search",
        "check.errors",
        "logs.grep_errors",
        "Need focused error evidence before deciding.",
    ))
    .step(DecisionPathStep::optional_observation(
        "ask.process_state",
        "pgrep.process_presence",
        "check.nodeplan_process",
        "proc.find_nodeplan",
        "Process evidence can help explain why the dry-run path is safe.",
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
    .ask(DecisionQuestionSpec::optional(
        "check.nodeplan_process",
        "proc.find_nodeplan",
        &pgrep_observation_tool(),
        "Is a nodeplan process already running?",
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

fn journal() -> DecisionRunbookJournal {
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

    DecisionRunbookJournal::new("nodeplan.bootstrap.journal")
        .append(&first, "grep found no blocking ERROR lines")
        .append(&second, "pgrep/process review allowed dry-run path")
}

#[test]
fn context_pack_selects_last_replay_steps_for_compact_context() {
    let journal = journal();

    let pack = DecisionContextPack::from_journal_selection(
        "nodeplan.bootstrap.context",
        "Decide what compact history should enter the next step.",
        &journal,
        DecisionContextPackReplaySelection::Last(1),
        512,
    )
    .expect("last-step selection should form");

    assert_eq!(pack.selected_replay_steps.len(), 1);
    assert_eq!(pack.selected_replay_steps[0].index, 1);
    assert!(pack.compact_context().contains("pgrep/process review allowed"));
    assert!(!pack.compact_context().contains("grep found no blocking ERROR lines"));
    assert_eq!(pack.digest(), pack.digest());
}

#[test]
fn context_pack_rejects_empty_or_out_of_bounds_selection() {
    let journal = journal();

    let empty = DecisionContextPack::from_journal_selection(
        "nodeplan.bootstrap.context",
        "empty selections should not be accepted",
        &journal,
        DecisionContextPackReplaySelection::Last(0),
        512,
    );
    assert!(matches!(empty, Err(DecisionContextPackError::EmptyReplaySelection)));

    let out_of_bounds = DecisionContextPack::from_journal_selection(
        "nodeplan.bootstrap.context",
        "bad range should not be accepted",
        &journal,
        DecisionContextPackReplaySelection::Range {
            start: 1,
            end_exclusive: 7,
        },
        512,
    );
    assert!(matches!(
        out_of_bounds,
        Err(DecisionContextPackError::ReplayRangeOutOfBounds { available: 2, .. })
    ));
}

#[test]
fn active_runbook_context_pack_links_source_questions_to_replay_context() {
    let plan = plan();
    let (manifest, material_contract) = material_contract(&plan);
    let guide = guide();
    let runbook = DecisionRunbookTemplate::observation_only(
        "nodeplan.active.runbook",
        "Use active source questions with recent replay context.",
        guide.clone(),
    );
    let instance = runbook
        .materialize(&plan, &manifest, &material_contract, "nodeplan.observations")
        .expect("runbook instance should materialize");
    let journal = journal();

    let pack = DecisionContextPack::for_active_runbook(
        "nodeplan.active.context",
        "Ask only the next useful questions.",
        &journal,
        &guide,
        &instance,
        DecisionContextPackReplaySelection::All,
        2048,
    )
    .expect("active context pack should form");

    assert_eq!(pack.selected_replay_steps.len(), 2);
    assert_eq!(pack.active_questions.len(), 2);
    assert_eq!(pack.active_runbook_contract_digest, Some(instance.contract.digest()));
    assert_eq!(pack.active_guide_digest, Some(guide.digest()));
    assert!(pack.compact_context().contains("Did grep find blocking ERROR lines?"));
    assert!(pack.compact_context().contains("optional check.nodeplan_process"));
    assert!(pack.fits_context_budget());
}

#[test]
fn context_pack_digest_tracks_replay_selection_questions_and_budget() {
    let plan = plan();
    let (manifest, material_contract) = material_contract(&plan);
    let guide = guide();
    let runbook = DecisionRunbookTemplate::observation_only(
        "nodeplan.active.runbook",
        "Use active source questions with recent replay context.",
        guide.clone(),
    );
    let instance = runbook
        .materialize(&plan, &manifest, &material_contract, "nodeplan.observations")
        .expect("runbook instance should materialize");
    let journal = journal();

    let all = DecisionContextPack::for_active_runbook(
        "nodeplan.active.context",
        "Ask only the next useful questions.",
        &journal,
        &guide,
        &instance,
        DecisionContextPackReplaySelection::All,
        2048,
    )
    .expect("active context pack should form");
    let last = DecisionContextPack::for_active_runbook(
        "nodeplan.active.context",
        "Ask only the next useful questions.",
        &journal,
        &guide,
        &instance,
        DecisionContextPackReplaySelection::Last(1),
        2048,
    )
    .expect("active context pack should form");
    let smaller_budget = DecisionContextPack::for_active_runbook(
        "nodeplan.active.context",
        "Ask only the next useful questions.",
        &journal,
        &guide,
        &instance,
        DecisionContextPackReplaySelection::All,
        64,
    )
    .expect("active context pack should form");

    assert_ne!(all.digest(), last.digest());
    assert_ne!(all.digest(), smaller_budget.digest());
    assert!(!smaller_budget.fits_context_budget());
}

#[test]
fn active_context_pack_rejects_wrong_source_guide_for_instance() {
    let plan = plan();
    let (manifest, material_contract) = material_contract(&plan);
    let guide = guide();
    let runbook = DecisionRunbookTemplate::observation_only(
        "nodeplan.active.runbook",
        "Use active source questions with recent replay context.",
        guide.clone(),
    );
    let instance = runbook
        .materialize(&plan, &manifest, &material_contract, "nodeplan.observations")
        .expect("runbook instance should materialize");
    let wrong_guide = DecisionGuideTemplate::new(
        "nodeplan.other.guide",
        "A different guide should not match this runbook instance.",
        DecisionPathTemplate::new("nodeplan.other.path", "different path"),
    );
    let journal = journal();

    let result = DecisionContextPack::for_active_runbook(
        "nodeplan.active.context",
        "Ask only the next useful questions.",
        &journal,
        &wrong_guide,
        &instance,
        DecisionContextPackReplaySelection::All,
        2048,
    );

    assert!(matches!(
        result,
        Err(DecisionContextPackError::SourceRunbookMismatch { .. })
    ));
}
