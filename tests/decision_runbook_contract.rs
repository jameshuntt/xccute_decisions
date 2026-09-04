use std::ffi::OsString;

use xccute_decisions::{
    DecisionConnectorSpec,
    DecisionGuideTemplate,
    DecisionPathStep,
    DecisionPathTemplate,
    DecisionQuestionSpec,
    DecisionRunbookMode,
    DecisionRunbookTemplate,
    DecisionRunbookTemplateError,
    grep_observation_tool,
};
use xccute_runtime::{
    RuntimeMaterialManifest,
    RuntimeOperation,
    RuntimeOperationPlan,
    RuntimePlanMaterialContract,
};

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

#[test]
fn runbook_links_guide_plan_material_contract_and_connector_call() {
    let plan = plan();
    let (manifest, material_contract) = material_contract(&plan);
    let runbook = DecisionRunbookTemplate::connector_execution(
        "nodeplan.bootstrap.runbook",
        "NodePlan bootstrap should ask focused questions before dry-run apply.",
        guide(),
        DecisionConnectorSpec::nodeplan(
            "nodeplan.local",
            "apply_dry_run",
            "nodeplan.apply_dry_run",
            "Run only after required observation evidence is available.",
        ),
    );

    let instance = runbook
        .materialize(&plan, &manifest, &material_contract, "nodeplan.observations")
        .expect("valid runbook materializes");

    assert_eq!(instance.contract.mode, DecisionRunbookMode::ConnectorExecution);
    assert_eq!(instance.contract.plan_digest, plan.digest());
    assert_eq!(
        instance.contract.material_contract_digest,
        material_contract.digest()
    );
    assert_eq!(
        instance.connector_call.as_ref().unwrap().operation_logical_id,
        "nodeplan.apply_dry_run"
    );
    assert_eq!(
        instance.contract.connector_call_digest,
        Some(instance.connector_call.as_ref().unwrap().digest())
    );
    assert_eq!(instance.runtime_guide.required_questions().count(), 1);
    assert_eq!(instance.observation_plan.requirements.len(), 1);
}

#[test]
fn observation_only_runbook_has_no_connector_call_but_still_links_materials() {
    let plan = plan();
    let (manifest, material_contract) = material_contract(&plan);
    let runbook = DecisionRunbookTemplate::observation_only(
        "nodeplan.observe.runbook",
        "Collect bounded evidence without launching connector execution.",
        guide(),
    );

    let instance = runbook
        .materialize(&plan, &manifest, &material_contract, "nodeplan.observations")
        .expect("observation-only runbook materializes");

    assert_eq!(instance.contract.mode, DecisionRunbookMode::ObservationOnly);
    assert!(instance.connector_call.is_none());
    assert!(instance.contract.connector_spec_digest.is_none());
    assert!(instance.contract.connector_call_digest.is_none());
}

#[test]
fn connector_execution_runbook_requires_start_operation_to_exist_in_plan() {
    let plan = plan();
    let (manifest, material_contract) = material_contract(&plan);
    let runbook = DecisionRunbookTemplate::connector_execution(
        "nodeplan.bad.runbook",
        "Bad runbook points to an operation not present in the verified plan.",
        guide(),
        DecisionConnectorSpec::nodeplan(
            "nodeplan.local",
            "apply_dry_run",
            "nodeplan.missing_operation",
            "This should be rejected.",
        ),
    );

    let err = runbook
        .materialize(&plan, &manifest, &material_contract, "nodeplan.observations")
        .unwrap_err();

    assert_eq!(
        err,
        DecisionRunbookTemplateError::StartOperationNotInPlan {
            operation_logical_id: "nodeplan.missing_operation".to_string()
        }
    );
}

#[test]
fn runbook_rejects_plan_material_contract_from_a_different_plan() {
    let plan = plan();
    let other_plan = RuntimeOperationPlan::new("other.plan")
        .then(op("logs.grep_errors", "grep", &["ERROR", "other.log"]));
    let manifest = RuntimeMaterialManifest::new("nodeplan.materials");
    let wrong_contract = RuntimePlanMaterialContract::new(&other_plan, &manifest);
    let runbook = DecisionRunbookTemplate::observation_only(
        "nodeplan.observe.runbook",
        "This should reject the mismatched plan/material link.",
        guide(),
    );

    let err = runbook
        .materialize(&plan, &manifest, &wrong_contract, "nodeplan.observations")
        .unwrap_err();

    assert!(matches!(
        err,
        DecisionRunbookTemplateError::MaterialContractPlanMismatch { .. }
    ));
}

#[test]
fn runbook_digest_tracks_connector_function_changes() {
    let runbook_a = DecisionRunbookTemplate::connector_execution(
        "nodeplan.bootstrap.runbook",
        "NodePlan bootstrap should ask focused questions before apply.",
        guide(),
        DecisionConnectorSpec::nodeplan(
            "nodeplan.local",
            "apply_dry_run",
            "nodeplan.apply_dry_run",
            "Run dry-run.",
        ),
    );
    let runbook_b = DecisionRunbookTemplate::connector_execution(
        "nodeplan.bootstrap.runbook",
        "NodePlan bootstrap should ask focused questions before apply.",
        guide(),
        DecisionConnectorSpec::nodeplan(
            "nodeplan.local",
            "commit_receipt",
            "nodeplan.apply_dry_run",
            "Commit receipt.",
        ),
    );

    assert_ne!(runbook_a.digest(), runbook_b.digest());
}
