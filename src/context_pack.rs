//! Adaptive context packs for decision runbook journals.
//!
//! Journals give us an append-only chain of compact runbook records. A context
//! pack selects the replay steps and active questions that should be carried
//! forward into the next decision. This is the decision-side contract for "use
//! bounded evidence, not a huge context dump."

use crate::guide::{DecisionGuideTemplate, DecisionQuestionSpec};
use crate::runbook::DecisionRunbookInstance;
use crate::runbook_journal::{DecisionRunbookJournal, DecisionRunbookReplayStep};
use xccute_runtime::StableDigest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionContextPackReplaySelection {
    All,
    Last(usize),
    Range { start: usize, end_exclusive: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionContextPackError {
    EmptyReplaySelection,
    ReplayRangeOutOfBounds {
        start: usize,
        end_exclusive: usize,
        available: usize,
    },
    SourceRunbookMismatch {
        template_source_digest: StableDigest,
        instance_source_digest: StableDigest,
    },
}

pub type DecisionContextPackResult<T> = Result<T, DecisionContextPackError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionContextPack {
    pub pack_id: String,
    pub goal: String,
    pub journal_id: String,
    pub journal_digest: StableDigest,
    pub selected_replay_steps: Vec<DecisionRunbookReplayStep>,
    pub active_runbook_contract_digest: Option<StableDigest>,
    pub active_guide_digest: Option<StableDigest>,
    pub active_questions: Vec<DecisionQuestionSpec>,
    pub context_budget_bytes: usize,
}

impl DecisionContextPack {
    pub fn from_journal(
        pack_id: impl Into<String>,
        goal: impl Into<String>,
        journal: &DecisionRunbookJournal,
        context_budget_bytes: usize,
    ) -> Self {
        Self::from_parts(
            pack_id,
            goal,
            journal,
            journal.replay_steps(),
            None,
            None,
            Vec::new(),
            context_budget_bytes,
        )
    }

    pub fn from_journal_selection(
        pack_id: impl Into<String>,
        goal: impl Into<String>,
        journal: &DecisionRunbookJournal,
        selection: DecisionContextPackReplaySelection,
        context_budget_bytes: usize,
    ) -> DecisionContextPackResult<Self> {
        let selected = select_replay_steps(journal, selection)?;
        Ok(Self::from_parts(
            pack_id,
            goal,
            journal,
            selected,
            None,
            None,
            Vec::new(),
            context_budget_bytes,
        ))
    }

    pub fn for_active_runbook(
        pack_id: impl Into<String>,
        goal: impl Into<String>,
        journal: &DecisionRunbookJournal,
        guide_template: &DecisionGuideTemplate,
        runbook_instance: &DecisionRunbookInstance,
        selection: DecisionContextPackReplaySelection,
        context_budget_bytes: usize,
    ) -> DecisionContextPackResult<Self> {
        let template_source_digest = guide_template.digest();
        let instance_guide_digest = runbook_instance.contract.guide_digest.clone();
        if template_source_digest != instance_guide_digest {
            return Err(DecisionContextPackError::SourceRunbookMismatch {
                template_source_digest,
                instance_source_digest: instance_guide_digest,
            });
        }

        let selected = select_replay_steps(journal, selection)?;
        Ok(Self::from_parts(
            pack_id,
            goal,
            journal,
            selected,
            Some(runbook_instance.contract.digest()),
            Some(runbook_instance.contract.guide_digest.clone()),
            guide_template.questions.clone(),
            context_budget_bytes,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn from_parts(
        pack_id: impl Into<String>,
        goal: impl Into<String>,
        journal: &DecisionRunbookJournal,
        selected_replay_steps: Vec<DecisionRunbookReplayStep>,
        active_runbook_contract_digest: Option<StableDigest>,
        active_guide_digest: Option<StableDigest>,
        active_questions: Vec<DecisionQuestionSpec>,
        context_budget_bytes: usize,
    ) -> Self {
        Self {
            pack_id: pack_id.into(),
            goal: goal.into(),
            journal_id: journal.journal_id.clone(),
            journal_digest: journal.digest(),
            selected_replay_steps,
            active_runbook_contract_digest,
            active_guide_digest,
            active_questions,
            context_budget_bytes,
        }
    }

    pub fn selected_context_bytes(&self) -> usize {
        let replay_bytes = self
            .selected_replay_steps
            .iter()
            .map(|step| step.compact_summary.len())
            .sum::<usize>();
        let question_bytes = self
            .active_questions
            .iter()
            .map(|question| {
                question.question.len() + question.responsibility.len()
            })
            .sum::<usize>();
        replay_bytes + question_bytes
    }

    pub fn fits_context_budget(&self) -> bool {
        self.selected_context_bytes() <= self.context_budget_bytes
    }

    pub fn compact_context(&self) -> String {
        let mut context = String::new();
        context.push_str("goal: ");
        context.push_str(&self.goal);
        context.push('\n');
        context.push_str("journal: ");
        context.push_str(&self.journal_id);
        context.push('\n');

        if !self.selected_replay_steps.is_empty() {
            context.push_str("replay:\n");
            for step in &self.selected_replay_steps {
                context.push_str("- ");
                context.push_str(&step.index.to_string());
                context.push_str(". ");
                context.push_str(&step.runbook_id);
                context.push_str(" [");
                context.push_str(step.mode.stable_label());
                context.push_str("]: ");
                context.push_str(&step.compact_summary);
                context.push('\n');
            }
        }

        if !self.active_questions.is_empty() {
            context.push_str("active_questions:\n");
            for question in &self.active_questions {
                let required = if question.required { "required" } else { "optional" };
                context.push_str("- ");
                context.push_str(required);
                context.push(' ');
                context.push_str(&question.logical_id);
                context.push_str(" via ");
                context.push_str(&question.tool_id);
                context.push('/');
                context.push_str(&question.operation_logical_id);
                context.push_str(": ");
                context.push_str(&question.question);
                context.push('\n');
            }
        }

        context
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.decisions.context.pack.v1\n");
        push_stable_field(&mut material, "pack_id", &self.pack_id);
        push_stable_field(&mut material, "goal", &self.goal);
        push_stable_field(&mut material, "journal_id", &self.journal_id);
        push_stable_field(&mut material, "journal_digest", self.journal_digest.as_str());
        push_stable_field(
            &mut material,
            "active_runbook_contract_digest",
            self.active_runbook_contract_digest
                .as_ref()
                .map(StableDigest::as_str)
                .unwrap_or(""),
        );
        push_stable_field(
            &mut material,
            "active_guide_digest",
            self.active_guide_digest
                .as_ref()
                .map(StableDigest::as_str)
                .unwrap_or(""),
        );
        push_stable_field(
            &mut material,
            "context_budget_bytes",
            &self.context_budget_bytes.to_string(),
        );
        for (index, step) in self.selected_replay_steps.iter().enumerate() {
            material.push_str("replay_step[");
            material.push_str(&index.to_string());
            material.push_str("]\n");
            push_stable_field(&mut material, "step.index", &step.index.to_string());
            push_stable_field(&mut material, "step.runbook_id", &step.runbook_id);
            push_stable_field(&mut material, "step.mode", step.mode.stable_label());
            push_stable_field(&mut material, "step.record_digest", step.record_digest.as_str());
            push_stable_field(&mut material, "step.compact_summary", &step.compact_summary);
        }
        for (index, question) in self.active_questions.iter().enumerate() {
            material.push_str("active_question[");
            material.push_str(&index.to_string());
            material.push_str("]\n");
            push_stable_field(&mut material, "question.digest", question.digest().as_str());
            material.push_str(&question.stable_material());
        }
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

fn select_replay_steps(
    journal: &DecisionRunbookJournal,
    selection: DecisionContextPackReplaySelection,
) -> DecisionContextPackResult<Vec<DecisionRunbookReplayStep>> {
    let steps = journal.replay_steps();
    let available = steps.len();

    match selection {
        DecisionContextPackReplaySelection::All => Ok(steps),
        DecisionContextPackReplaySelection::Last(count) => {
            if count == 0 {
                return Err(DecisionContextPackError::EmptyReplaySelection);
            }
            let start = available.saturating_sub(count);
            Ok(steps[start..].to_vec())
        }
        DecisionContextPackReplaySelection::Range {
            start,
            end_exclusive,
        } => {
            if start >= end_exclusive {
                return Err(DecisionContextPackError::EmptyReplaySelection);
            }
            if end_exclusive > available {
                return Err(DecisionContextPackError::ReplayRangeOutOfBounds {
                    start,
                    end_exclusive,
                    available,
                });
            }
            Ok(steps[start..end_exclusive].to_vec())
        }
    }
}

fn push_stable_field(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push_str(".len=");
    out.push_str(&value.len().to_string());
    out.push('\n');
    out.push_str(key);
    out.push('=');
    out.push_str(value);
    out.push('\n');
}
