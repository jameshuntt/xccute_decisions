use xccute_decisions::{
    DecisionContextPack,
    DecisionContextRequest,
    DecisionContextRequestError,
    DecisionContextRequestSelection,
    DecisionQuestionSpec,
    DecisionRunbookMode,
    DecisionRunbookReplayStep,
    grep_observation_tool,
    pgrep_observation_tool,
};
use xccute_runtime::StableDigest;

fn digest(label: &str) -> StableDigest {
    StableDigest::sha256(label)
}

fn active_pack() -> DecisionContextPack {
    DecisionContextPack {
        pack_id: "nodeplan.active.context".to_string(),
        goal: "Ask only focused questions before the next dry-run decision.".to_string(),
        journal_id: "nodeplan.bootstrap.journal".to_string(),
        journal_digest: digest("journal.digest"),
        selected_replay_steps: vec![DecisionRunbookReplayStep {
            index: 0,
            runbook_id: "nodeplan.error_review.runbook".to_string(),
            mode: DecisionRunbookMode::ObservationOnly,
            record_digest: digest("record.digest"),
            compact_summary: "grep found no blocking ERROR lines".to_string(),
        }],
        active_runbook_contract_digest: Some(digest("runbook.contract")),
        active_guide_digest: Some(digest("guide.digest")),
        active_questions: vec![
            DecisionQuestionSpec::required(
                "check.errors",
                "logs.grep_errors",
                &grep_observation_tool(),
                "Did grep find blocking ERROR lines?",
            ),
            DecisionQuestionSpec::optional(
                "check.nodeplan_process",
                "proc.find_nodeplan",
                &pgrep_observation_tool(),
                "Is a nodeplan process already running?",
            ),
        ],
        context_budget_bytes: 2048,
    }
}

#[test]
fn context_request_selects_required_question_from_compact_pack() {
    let pack = active_pack();

    let request = DecisionContextRequest::required_from_context_pack(
        "nodeplan.next.request",
        "Ask the minimum required evidence question.",
        &pack,
    )
    .expect("required request should form");

    assert_eq!(request.selected_question_ids(), vec!["check.errors".to_string()]);
    assert_eq!(request.context_pack_digest, pack.digest());
    assert_eq!(request.journal_digest, pack.journal_digest);
    assert_eq!(request.carried_replay_summaries.len(), 1);
    assert!(request.compact_request().contains("grep found no blocking ERROR lines"));
    assert!(request.compact_request().contains("Did grep find blocking ERROR lines?"));
    assert!(!request.compact_request().contains("Is a nodeplan process already running?"));
}

#[test]
fn context_request_can_choose_named_optional_questions_without_raw_context_expansion() {
    let pack = active_pack();

    let request = DecisionContextRequest::from_context_pack(
        "nodeplan.process.request",
        "Ask a focused optional process question only if it helps the next step.",
        &pack,
        DecisionContextRequestSelection::ByLogicalIds(vec![
            "check.nodeplan_process".to_string(),
        ]),
        1,
    )
    .expect("named optional request should form");

    assert_eq!(request.selected_question_ids(), vec!["check.nodeplan_process".to_string()]);
    assert_eq!(request.selected_context_budget_hint(), 512);
    let compact = request.compact_request();
    assert!(compact.contains("pgrep.process_search"));
    assert!(compact.contains("proc.find_nodeplan"));
    assert!(!compact.contains("Did grep find blocking ERROR lines?"));
}

#[test]
fn context_request_rejects_bad_or_underfilled_question_selections() {
    let pack = active_pack();

    let empty = DecisionContextRequest::from_context_pack(
        "nodeplan.empty.request",
        "empty selections are not useful",
        &pack,
        DecisionContextRequestSelection::First(0),
        1,
    );
    assert!(matches!(empty, Err(DecisionContextRequestError::EmptyQuestionSelection)));

    let missing = DecisionContextRequest::from_context_pack(
        "nodeplan.missing.request",
        "unknown questions should not be invented",
        &pack,
        DecisionContextRequestSelection::ByLogicalIds(vec!["check.unknown".to_string()]),
        1,
    );
    assert!(matches!(
        missing,
        Err(DecisionContextRequestError::QuestionNotInContext { logical_id })
            if logical_id == "check.unknown"
    ));

    let duplicate = DecisionContextRequest::from_context_pack(
        "nodeplan.duplicate.request",
        "question ids must be unique",
        &pack,
        DecisionContextRequestSelection::ByLogicalIds(vec![
            "check.errors".to_string(),
            "check.errors".to_string(),
        ]),
        1,
    );
    assert!(matches!(
        duplicate,
        Err(DecisionContextRequestError::DuplicateQuestionSelection { logical_id })
            if logical_id == "check.errors"
    ));

    let underfilled = DecisionContextRequest::from_context_pack(
        "nodeplan.underfilled.request",
        "minimum observation counts must be explicit",
        &pack,
        DecisionContextRequestSelection::First(1),
        2,
    );
    assert!(matches!(
        underfilled,
        Err(DecisionContextRequestError::MinimumObservationCountUnsatisfied {
            minimum_required: 2,
            selected_count: 1,
        })
    ));
}

#[test]
fn context_request_digest_tracks_selected_question_order() {
    let pack = active_pack();

    let errors_then_process = DecisionContextRequest::from_context_pack(
        "nodeplan.ordered.request",
        "Question order is part of the contract.",
        &pack,
        DecisionContextRequestSelection::ByLogicalIds(vec![
            "check.errors".to_string(),
            "check.nodeplan_process".to_string(),
        ]),
        1,
    )
    .expect("ordered request should form");

    let process_then_errors = DecisionContextRequest::from_context_pack(
        "nodeplan.ordered.request",
        "Question order is part of the contract.",
        &pack,
        DecisionContextRequestSelection::ByLogicalIds(vec![
            "check.nodeplan_process".to_string(),
            "check.errors".to_string(),
        ]),
        1,
    )
    .expect("reordered request should form");

    assert_ne!(errors_then_process.digest(), process_then_errors.digest());
}

#[test]
fn context_request_materializes_runtime_observation_requirements() {
    let pack = active_pack();

    let request = DecisionContextRequest::from_context_pack(
        "nodeplan.next.request",
        "Ask both focused questions.",
        &pack,
        DecisionContextRequestSelection::First(2),
        2,
    )
    .expect("two-question request should form");

    let requirements = request.to_observation_requirements();
    assert_eq!(requirements.len(), 2);
    assert_eq!(requirements[0].logical_id.as_str(), "check.errors");
    assert!(requirements[0].required);
    assert_eq!(requirements[1].logical_id.as_str(), "check.nodeplan_process");
    assert!(!requirements[1].required);
}

#[test]
fn context_request_rejects_context_packs_without_active_questions() {
    let mut pack = active_pack();
    pack.active_questions.clear();

    let request = DecisionContextRequest::from_context_pack(
        "nodeplan.no_questions.request",
        "a request needs at least one active question source",
        &pack,
        DecisionContextRequestSelection::RequiredOnly,
        1,
    );

    assert!(matches!(
        request,
        Err(DecisionContextRequestError::NoActiveQuestionsInContextPack)
    ));
}
