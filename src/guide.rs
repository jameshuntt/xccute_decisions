//! Source-level guided decision templates.
//!
//! Runtime receipts prove what happened. This module owns the decision-side
//! source contract: what goal is being pursued, which focused questions may be
//! asked, which observations are required, which ones are optional, and how that
//! source contract materializes into runtime observation requirements.

use crate::observation_tool::DecisionObservationTool;
use crate::path::{DecisionPathStepKind, DecisionPathTemplate};
use xccute_runtime::{
    RuntimeDecisionGuide,
    RuntimeDecisionQuestion,
    RuntimeObservationKind,
    RuntimeObservationPlan,
    RuntimeOperationPlan,
    StableDigest,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionQuestionSpec {
    pub logical_id: String,
    pub operation_logical_id: String,
    pub tool_id: String,
    pub kind: RuntimeObservationKind,
    pub question: String,
    pub responsibility: String,
    pub required: bool,
    pub minimum_facts: usize,
    pub context_budget_hint: usize,
}

impl DecisionQuestionSpec {
    pub fn required(
        logical_id: impl Into<String>,
        operation_logical_id: impl Into<String>,
        tool: &DecisionObservationTool,
        question: impl Into<String>,
    ) -> Self {
        Self::from_tool(logical_id, operation_logical_id, tool, question, true)
    }

    pub fn optional(
        logical_id: impl Into<String>,
        operation_logical_id: impl Into<String>,
        tool: &DecisionObservationTool,
        question: impl Into<String>,
    ) -> Self {
        Self::from_tool(logical_id, operation_logical_id, tool, question, false)
    }

    fn from_tool(
        logical_id: impl Into<String>,
        operation_logical_id: impl Into<String>,
        tool: &DecisionObservationTool,
        question: impl Into<String>,
        required: bool,
    ) -> Self {
        Self {
            logical_id: logical_id.into(),
            operation_logical_id: operation_logical_id.into(),
            tool_id: tool.tool_id.to_string(),
            kind: tool.kind.clone(),
            question: question.into(),
            responsibility: tool.responsibility.to_string(),
            required,
            minimum_facts: 1,
            context_budget_hint: 512,
        }
    }

    pub fn with_minimum_facts(mut self, minimum_facts: usize) -> Self {
        self.minimum_facts = minimum_facts;
        self
    }

    pub fn with_context_budget_hint(mut self, bytes: usize) -> Self {
        self.context_budget_hint = bytes;
        self
    }

    pub fn materialize(&self) -> RuntimeDecisionQuestion {
        let question = if self.required {
            RuntimeDecisionQuestion::required(
                self.logical_id.clone(),
                self.operation_logical_id.clone(),
                self.kind.clone(),
                self.question.clone(),
                self.responsibility.clone(),
            )
        } else {
            RuntimeDecisionQuestion::optional(
                self.logical_id.clone(),
                self.operation_logical_id.clone(),
                self.kind.clone(),
                self.question.clone(),
                self.responsibility.clone(),
            )
        };

        question
            .with_minimum_facts(self.minimum_facts)
            .with_context_budget_hint(self.context_budget_hint)
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.decisions.question.spec.v1\n");
        push_stable_field(&mut material, "logical_id", &self.logical_id);
        push_stable_field(&mut material, "operation_logical_id", &self.operation_logical_id);
        push_stable_field(&mut material, "tool_id", &self.tool_id);
        push_stable_field(&mut material, "kind", &self.kind.stable_label());
        push_stable_field(&mut material, "question", &self.question);
        push_stable_field(&mut material, "responsibility", &self.responsibility);
        push_stable_field(&mut material, "required", &self.required.to_string());
        push_stable_field(&mut material, "minimum_facts", &self.minimum_facts.to_string());
        push_stable_field(
            &mut material,
            "context_budget_hint",
            &self.context_budget_hint.to_string(),
        );
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionGuideTemplateError {
    MissingRequiredPathQuestions { missing_question_ids: Vec<String> },
    MissingRequiredPathOperations { missing_operation_ids: Vec<String> },
    UnexpectedRequiredQuestion { question_ids: Vec<String> },
}

pub type DecisionGuideTemplateResult<T> = Result<T, DecisionGuideTemplateError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionGuideTemplate {
    pub guide_id: String,
    pub goal: String,
    pub path: DecisionPathTemplate,
    pub questions: Vec<DecisionQuestionSpec>,
}

impl DecisionGuideTemplate {
    pub fn new(
        guide_id: impl Into<String>,
        goal: impl Into<String>,
        path: DecisionPathTemplate,
    ) -> Self {
        Self {
            guide_id: guide_id.into(),
            goal: goal.into(),
            path,
            questions: Vec::new(),
        }
    }

    pub fn ask(mut self, question: DecisionQuestionSpec) -> Self {
        self.questions.push(question);
        self
    }

    pub fn validate_path_coverage(&self) -> DecisionGuideTemplateResult<()> {
        let mut missing_question_ids = Vec::new();
        let mut missing_operation_ids = Vec::new();

        for step in &self.path.steps {
            if step.kind == DecisionPathStepKind::RequiredObservation {
                match (&step.question_logical_id, &step.operation_logical_id) {
                    (Some(question_id), Some(operation_id)) => {
                        let covered = self.questions.iter().any(|question| {
                            question.required
                                && question.logical_id.as_str() == question_id.as_str()
                                && question.operation_logical_id.as_str() == operation_id.as_str()
                        });
                        if !covered {
                            missing_question_ids.push(question_id.clone());
                        }
                    }
                    (Some(question_id), None) => missing_operation_ids.push(question_id.clone()),
                    (None, Some(operation_id)) => missing_question_ids.push(operation_id.clone()),
                    (None, None) => missing_question_ids.push(step.step_id.clone()),
                }
            }
        }

        if !missing_question_ids.is_empty() {
            return Err(DecisionGuideTemplateError::MissingRequiredPathQuestions {
                missing_question_ids,
            });
        }

        if !missing_operation_ids.is_empty() {
            return Err(DecisionGuideTemplateError::MissingRequiredPathOperations {
                missing_operation_ids,
            });
        }

        let unexpected_required = self
            .questions
            .iter()
            .filter(|question| question.required)
            .filter(|question| !self.path.includes_required_observation(&question.logical_id))
            .map(|question| question.logical_id.clone())
            .collect::<Vec<_>>();

        if !unexpected_required.is_empty() {
            return Err(DecisionGuideTemplateError::UnexpectedRequiredQuestion {
                question_ids: unexpected_required,
            });
        }

        Ok(())
    }

    pub fn materialize_runtime_guide(
        &self,
        plan: &RuntimeOperationPlan,
    ) -> DecisionGuideTemplateResult<RuntimeDecisionGuide> {
        self.validate_path_coverage()?;

        let mut guide = RuntimeDecisionGuide::new(self.guide_id.clone(), self.goal.clone(), plan);
        for question in &self.questions {
            guide = guide.ask(question.materialize());
        }
        Ok(guide)
    }

    pub fn materialize_observation_plan(
        &self,
        plan: &RuntimeOperationPlan,
        observation_plan_id: impl Into<String>,
    ) -> DecisionGuideTemplateResult<RuntimeObservationPlan> {
        Ok(self
            .materialize_runtime_guide(plan)?
            .to_observation_plan(observation_plan_id))
    }

    pub fn required_questions(&self) -> impl Iterator<Item = &DecisionQuestionSpec> {
        self.questions.iter().filter(|question| question.required)
    }

    pub fn optional_questions(&self) -> impl Iterator<Item = &DecisionQuestionSpec> {
        self.questions.iter().filter(|question| !question.required)
    }

    pub fn stable_material(&self) -> String {
        let mut material = String::new();
        material.push_str("xccute.decisions.guide.template.v1\n");
        push_stable_field(&mut material, "guide_id", &self.guide_id);
        push_stable_field(&mut material, "goal", &self.goal);
        push_stable_field(&mut material, "path_digest", self.path.digest().as_str());
        material.push_str(&self.path.stable_material());
        for (index, question) in self.questions.iter().enumerate() {
            material.push_str("question[");
            material.push_str(&index.to_string());
            material.push_str("].digest=");
            material.push_str(question.digest().as_str());
            material.push('\n');
            material.push_str(&question.stable_material());
        }
        material
    }

    pub fn digest(&self) -> StableDigest {
        StableDigest::sha256(self.stable_material())
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
