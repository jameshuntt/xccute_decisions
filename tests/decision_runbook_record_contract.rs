use std::ffi::OsString;

use xccute_decisions::{
    DecisionConnectorSpec,
    DecisionGuideTemplate,
    DecisionPathStep,
    DecisionPathTemplate,
    DecisionQuestionSpec,
    DecisionRunbookRecord,
    DecisionRunbookRecordError,
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

fn connector_runbook() -> DecisionRunbookTemplate {
    DecisionRunbookTemplate::connector_execution(
        "nodeplan.bootstrap.runbook",
        "NodePlan bootstrap should ask focused questions before dry-run apply.",
        guide(),
        DecisionConnectorSpec::nodeplan(
            "nodeplan.local",
            "apply_dry_run",
            "nodeplan.apply_dry_run",
            "Run only after required observation evidence is available.",
        ),
    )
}

fn observation_runbook() -> DecisionRunbookTemplate {
    DecisionRunbookTemplate::observation_only(
        "nodeplan.observation.runbook",
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

fn execution_receipt(
    plan: &RuntimeOperationPlan,
    manifest: &RuntimeMaterialManifest,
    material_contract: &RuntimePlanMaterialContract,
    instance: &xccute_decisions::DecisionRunbookInstance,
) -> RuntimeConnectorExecutionReceipt {
    let call = instance
        .connector_call
        .as_ref()
        .expect("connector call exists")
        .clone();
    let intent = RuntimeConnectorExecutionIntent::prepare(call.clone(), material_contract, manifest)
        .expect("empty material manifest should satisfy gate");
    let operation = plan
        .operation_by_logical_id("nodeplan.apply_dry_run")
        .expect("apply operation exists");
    let decision = ExitStatusPolicy::new()
        .with_rule(ExitStatusRule::accepted_code(
            0,
            ExitDisposition::Continue,
            "dry-run accepted: commit receipt",
        ))
        .decide(RuntimeExitStatus::new(Some(0), true));
    let connector_observation = RuntimeConnectorObservation::new(
        &call,
        RuntimeExitStatus::new(Some(0), true),
        "nodeplan dry-run accepted",
    );
    let connector_receipt = RuntimeConnectorReceipt::from_decision(
        call,
        connector_observation,
        plan,
        operation,
        decision,
    )
    .expect("connector receipt should form");

    RuntimeConnectorExecutionReceipt::new(&intent, connector_receipt)
}

fn wrong_execution_receipt(
    plan: &RuntimeOperationPlan,
    manifest: &RuntimeMaterialManifest,
    material_contract: &RuntimePlanMaterialContract,
) -> RuntimeConnectorExecutionReceipt {
    let operation = plan
        .operation_by_logical_id("logs.grep_errors")
        .expect("grep operation exists");
    let call = RuntimeConnectorCall::for_operation(
        RuntimeConnectorIdentity::new("nodeplan.local", "nodeplan"),
        "nodeplan-control",
        "grep_errors",
        plan,
        operation,
        "wrong receipt for runbook start operation",
    )
    .expect("call should form");
    let intent = RuntimeConnectorExecutionIntent::prepare(call.clone(), material_contract, manifest)
        .expect("empty material manifest should satisfy gate");
    let decision = ExitStatusPolicy::new()
        .with_rule(ExitStatusRule::accepted_code(
            0,
            ExitDisposition::Continue,
            "grep completed: continue",
        ))
        .decide(RuntimeExitStatus::new(Some(0), true));
    let connector_observation = RuntimeConnectorObservation::new(
        &call,
        RuntimeExitStatus::new(Some(0), true),
        "grep completed",
    );
    let connector_receipt = RuntimeConnectorReceipt::from_decision(
        call,
        connector_observation,
        plan,
        operation,
        decision,
    )
    .expect("connector receipt should form");

    RuntimeConnectorExecutionReceipt::new(&intent, connector_receipt)
}

#[test]
fn runbook_record_links_context_execution_observation_receipt_and_trace() {
    let plan = plan();
    let (manifest, material_contract) = material_contract(&plan);
    let instance = connector_runbook()
        .materialize(&plan, &manifest, &material_contract, "nodeplan.observations")
        .expect("runbook should materialize");
    let context = observation_context(&plan, &manifest, &material_contract, &instance);
    let execution_receipt = execution_receipt(&plan, &manifest, &material_contract, &instance);
    let observation_receipt = RuntimeObservationReceipt::new(
        &execution_receipt,
        &instance.observation_plan,
        &RuntimeObservationEvidenceSet {
            plan_digest: plan.digest(),
            facts: Vec::new(),
        },
    );
    assert!(matches!(
        observation_receipt,
        Err(RuntimeObservationError::MissingRequiredObservations { .. })
    ));

    let requirement = instance.runtime_guide.questions[0].to_observation_requirement();
    let grep_operation = plan.operation_by_logical_id("logs.grep_errors").unwrap();
    let observation_call = RuntimeObservationCall::for_intent(
        &requirement,
        &RuntimeConnectorExecutionIntent::prepare(
            RuntimeConnectorCall::for_operation(
                RuntimeConnectorIdentity::new("nodeplan.local", "nodeplan"),
                "nodeplan-control",
                "grep_errors",
                &plan,
                grep_operation,
                "collect bounded error evidence",
            )
            .unwrap(),
            &material_contract,
            &manifest,
        )
        .unwrap(),
        "check whether errors block dry-run apply",
    )
    .unwrap();
    let fact = RuntimeObservationFact::from_text(
        &observation_call,
        "fact.error_pattern",
        false,
        0,
        "",
        "grep found 0 ERROR lines",
        "no blocking error evidence, so continue",
    );
    let evidence = RuntimeObservationEvidenceSet::new(&plan).with_fact(fact);
    let observation_receipt = RuntimeObservationReceipt::new(
        &execution_receipt,
        &instance.observation_plan,
        &evidence,
    )
    .expect("required observation should create receipt");
    let acknowledged = RuntimeAcknowledgedDecisionPath::new(
        &context,
        &execution_receipt.connector_receipt.transition,
        "bounded evidence and dry-run exit status justify commit receipt",
    );
    let trace = RuntimeExecutionTrace::new("nodeplan.bootstrap.trace").append(
        &execution_receipt,
        Some(&observation_receipt),
        "runbook accepted the dry-run transition",
    );

    let record = DecisionRunbookRecord::connector_execution(
        &instance,
        &context,
        &acknowledged,
        &execution_receipt,
        Some(&observation_receipt),
        Some(&trace),
        "source runbook, compact evidence, and execution receipt are linked",
    )
    .expect("valid record should form");

    assert_eq!(record.runbook_id, "nodeplan.bootstrap.runbook");
    assert_eq!(record.contract_digest, instance.contract.digest());
    assert_eq!(record.execution_receipt_digest, Some(execution_receipt.digest()));
    assert_eq!(record.observation_receipt_digest, Some(observation_receipt.digest()));
    assert_eq!(record.execution_trace_digest, Some(trace.digest()));
    assert_eq!(record.digest(), record.digest());
}

#[test]
fn runbook_record_rejects_wrong_connector_execution_receipt() {
    let plan = plan();
    let (manifest, material_contract) = material_contract(&plan);
    let instance = connector_runbook()
        .materialize(&plan, &manifest, &material_contract, "nodeplan.observations")
        .expect("runbook should materialize");
    let context = observation_context(&plan, &manifest, &material_contract, &instance);
    let wrong_receipt = wrong_execution_receipt(&plan, &manifest, &material_contract);
    let acknowledged = RuntimeAcknowledgedDecisionPath::new(
        &context,
        &wrong_receipt.connector_receipt.transition,
        "this is the wrong transition for the runbook start call",
    );

    let result = DecisionRunbookRecord::connector_execution(
        &instance,
        &context,
        &acknowledged,
        &wrong_receipt,
        None,
        None,
        "should fail because receipt belongs to grep, not dry-run apply",
    );

    assert!(matches!(
        result,
        Err(DecisionRunbookRecordError::ConnectorCallMismatch { .. })
    ));
}

#[test]
fn runbook_record_rejects_trace_that_does_not_include_expected_observation_receipt() {
    let plan = plan();
    let (manifest, material_contract) = material_contract(&plan);
    let instance = connector_runbook()
        .materialize(&plan, &manifest, &material_contract, "nodeplan.observations")
        .expect("runbook should materialize");
    let context = observation_context(&plan, &manifest, &material_contract, &instance);
    let execution_receipt = execution_receipt(&plan, &manifest, &material_contract, &instance);
    let requirement = instance.runtime_guide.questions[0].to_observation_requirement();
    let grep_operation = plan.operation_by_logical_id("logs.grep_errors").unwrap();
    let observation_call = RuntimeObservationCall::for_intent(
        &requirement,
        &RuntimeConnectorExecutionIntent::prepare(
            RuntimeConnectorCall::for_operation(
                RuntimeConnectorIdentity::new("nodeplan.local", "nodeplan"),
                "nodeplan-control",
                "grep_errors",
                &plan,
                grep_operation,
                "collect bounded error evidence",
            )
            .unwrap(),
            &material_contract,
            &manifest,
        )
        .unwrap(),
        "check whether errors block dry-run apply",
    )
    .unwrap();
    let fact = RuntimeObservationFact::from_text(
        &observation_call,
        "fact.error_pattern",
        false,
        0,
        "",
        "grep found 0 ERROR lines",
        "no blocking error evidence, so continue",
    );
    let evidence = RuntimeObservationEvidenceSet::new(&plan).with_fact(fact);
    let observation_receipt = RuntimeObservationReceipt::new(
        &execution_receipt,
        &instance.observation_plan,
        &evidence,
    )
    .expect("required observation should create receipt");
    let acknowledged = RuntimeAcknowledgedDecisionPath::new(
        &context,
        &execution_receipt.connector_receipt.transition,
        "bounded evidence and dry-run exit status justify commit receipt",
    );
    let trace_without_observation = RuntimeExecutionTrace::new("nodeplan.bootstrap.trace")
        .append(&execution_receipt, None, "missing observation receipt");

    let result = DecisionRunbookRecord::connector_execution(
        &instance,
        &context,
        &acknowledged,
        &execution_receipt,
        Some(&observation_receipt),
        Some(&trace_without_observation),
        "should fail because the trace omitted the observation receipt",
    );

    assert!(matches!(
        result,
        Err(DecisionRunbookRecordError::TraceObservationMismatch { .. })
    ));
}

#[test]
fn observation_only_runbook_record_has_no_connector_execution_receipt() {
    let plan = plan();
    let (manifest, material_contract) = material_contract(&plan);
    let instance = observation_runbook()
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
        "operator acknowledged the observation-only path",
    );

    let record = DecisionRunbookRecord::observation_only(
        &instance,
        &context,
        &acknowledged,
        "observation-only runbook captured compact evidence without executing primary connector",
    )
    .expect("observation-only record should form");

    assert!(record.execution_receipt_digest.is_none());
    assert!(record.observation_receipt_digest.is_none());
    assert!(record.execution_trace_digest.is_none());
}

#[test]
fn observation_only_record_rejects_connector_execution_runbook() {
    let plan = plan();
    let (manifest, material_contract) = material_contract(&plan);
    let instance = connector_runbook()
        .materialize(&plan, &manifest, &material_contract, "nodeplan.observations")
        .expect("runbook should materialize");
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
        "operator acknowledged the observation-only path",
    );

    let result = DecisionRunbookRecord::observation_only(
        &instance,
        &context,
        &acknowledged,
        "should fail because this runbook has a primary connector call",
    );

    assert!(matches!(
        result,
        Err(DecisionRunbookRecordError::ExpectedObservationOnlyRunbook { .. })
    ));
}
