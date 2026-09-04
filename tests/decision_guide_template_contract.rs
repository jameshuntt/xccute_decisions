use std::ffi::OsString;

use xccute_decisions::{
    grep_observation_tool,
    pgrep_observation_tool,
    DecisionGuideTemplate,
    DecisionGuideTemplateError,
    DecisionPathStep,
    DecisionPathTemplate,
    DecisionQuestionSpec,
};
use xccute_runtime::{RuntimeOperation, RuntimeOperationPlan};

fn plan() -> RuntimeOperationPlan {
    RuntimeOperationPlan::new("fleet.review")
        .then(RuntimeOperation::new(
            "logs.grep_errors",
            OsString::from("grep"),
            vec![OsString::from("ERROR"), OsString::from("fleet.log")],
            "grep ERROR fleet.log",
        ))
        .then(RuntimeOperation::new(
            "fleet.commit_receipt",
            OsString::from("fleetctl"),
            vec![OsString::from("commit-receipt")],
            "fleetctl commit-receipt",
        ))
}

#[test]
fn decision_guide_template_materializes_runtime_guide_and_observation_plan() {
    let path = DecisionPathTemplate::new(
        "fleet.error_review",
        "Only continue if no blocking error evidence exists.",
    )
    .step(DecisionPathStep::required_observation(
        "ask.error_pattern",
        "grep.pattern_search",
        "check.errors",
        "logs.grep_errors",
        "Need one focused grep observation before deciding.",
    ))
    .step(DecisionPathStep::operation(
        "commit.receipt",
        "fleet.commit_receipt",
        "Commit only after an evidence-backed decision.",
    ));

    let guide = DecisionGuideTemplate::new(
        "fleet.error_review.guide",
        "Decide whether the Supervisor dry-run log is safe to commit.",
        path,
    )
    .ask(DecisionQuestionSpec::required(
        "check.errors",
        "logs.grep_errors",
        &grep_observation_tool(),
        "Did grep find blocking ERROR lines?",
    ));

    let runtime_guide = guide.materialize_runtime_guide(&plan()).expect("runtime guide");
    let observation_plan = guide
        .materialize_observation_plan(&plan(), "fleet.error_review.observations")
        .expect("observation plan");

    assert_eq!(runtime_guide.guide_id, "fleet.error_review.guide");
    assert_eq!(runtime_guide.questions.len(), 1);
    assert_eq!(observation_plan.requirements.len(), 1);
    assert_eq!(observation_plan.requirements[0].logical_id, "check.errors");
}

#[test]
fn decision_guide_template_rejects_missing_required_path_question() {
    let path = DecisionPathTemplate::new("fleet.error_review", "goal")
        .step(DecisionPathStep::required_observation(
            "ask.error_pattern",
            "grep.pattern_search",
            "check.errors",
            "logs.grep_errors",
            "Need the grep observation.",
        ));

    let guide = DecisionGuideTemplate::new("guide", "goal", path);

    let err = guide.materialize_runtime_guide(&plan()).unwrap_err();
    assert_eq!(
        err,
        DecisionGuideTemplateError::MissingRequiredPathQuestions {
            missing_question_ids: vec!["check.errors".to_string()],
        }
    );
}

#[test]
fn decision_guide_template_allows_optional_questions_without_making_them_required() {
    let path = DecisionPathTemplate::new("envd.probe", "Probe before selecting setup path.")
        .step(DecisionPathStep::required_observation(
            "ask.config",
            "grep.pattern_search",
            "check.config",
            "logs.grep_errors",
            "Need config evidence first.",
        ))
        .step(DecisionPathStep::optional_observation(
            "ask.worker",
            "pgrep.process_search",
            "check.worker",
            "fleet.pgrep_worker",
            "Optionally see if a worker is already running.",
        ));

    let guide = DecisionGuideTemplate::new("envd.probe.guide", "Probe with bounded evidence.", path)
        .ask(DecisionQuestionSpec::required(
            "check.config",
            "logs.grep_errors",
            &grep_observation_tool(),
            "Did config evidence exist?",
        ))
        .ask(DecisionQuestionSpec::optional(
            "check.worker",
            "fleet.pgrep_worker",
            &pgrep_observation_tool(),
            "Was the worker already running?",
        ));

    assert_eq!(guide.required_questions().count(), 1);
    assert_eq!(guide.optional_questions().count(), 1);
    assert!(guide.materialize_runtime_guide(&plan()).is_ok());
}

#[test]
fn decision_guide_template_digest_tracks_source_path_and_questions() {
    let path = DecisionPathTemplate::new("path", "goal")
        .step(DecisionPathStep::required_observation(
            "ask.a",
            "grep.pattern_search",
            "q.a",
            "op.a",
            "first",
        ));

    let first = DecisionGuideTemplate::new("guide", "goal", path.clone())
        .ask(DecisionQuestionSpec::required(
            "q.a",
            "op.a",
            &grep_observation_tool(),
            "question a",
        ));

    let second = DecisionGuideTemplate::new("guide", "goal", path)
        .ask(DecisionQuestionSpec::required(
            "q.a",
            "op.a",
            &grep_observation_tool(),
            "question a changed",
        ));

    assert_ne!(first.digest(), second.digest());
}
