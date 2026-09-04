use xccute_decisions::{DecisionPathStep, DecisionPathTemplate};

#[test]
fn decision_path_models_required_observation_before_operation() {
    let path = DecisionPathTemplate::new(
        "fleet.error_review",
        "Only continue if the logs do not contain blocking error evidence.",
    )
    .step(DecisionPathStep::required_observation(
        "ask.error_pattern",
        "grep.pattern_search",
        "check.errors",
        "logs.grep_errors",
        "Need one focused observation before deciding whether to continue.",
    ))
    .step(DecisionPathStep::operation(
        "commit.receipt",
        "fleet.commit_receipt",
        "Commit only after evidence-backed decision context exists.",
    ));

    assert_eq!(path.required_observation_count(), 1);
    assert!(path.includes_required_observation("check.errors"));
}

#[test]
fn decision_path_can_include_optional_observation_branches() {
    let path = DecisionPathTemplate::new("envd.probe", "Probe environment before selecting a setup path.")
        .step(DecisionPathStep::required_observation(
            "ask.config",
            "grep.pattern_search",
            "check.config",
            "envd.grep_config",
            "Find whether the config already names the desired runtime.",
        ))
        .step(DecisionPathStep::optional_observation(
            "ask.process",
            "pgrep.process_search",
            "check.worker",
            "envd.pgrep_worker",
            "Optionally see if a worker is already running before starting one.",
        ))
        .step(DecisionPathStep::branch(
            "branch.worker_state",
            "Use compact evidence to choose start, restart, or skip.",
        ));

    assert_eq!(path.required_observation_count(), 1);
    assert_eq!(path.optional_steps().count(), 1);
}

#[test]
fn decision_path_digest_is_order_sensitive() {
    let first = DecisionPathTemplate::new("path", "goal")
        .step(DecisionPathStep::required_observation(
            "ask.a",
            "grep.pattern_search",
            "q.a",
            "op.a",
            "first",
        ))
        .step(DecisionPathStep::operation("op.b", "operation.b", "second"));

    let second = DecisionPathTemplate::new("path", "goal")
        .step(DecisionPathStep::operation("op.b", "operation.b", "second"))
        .step(DecisionPathStep::required_observation(
            "ask.a",
            "grep.pattern_search",
            "q.a",
            "op.a",
            "first",
        ));

    assert_ne!(first.digest(), second.digest());
}
