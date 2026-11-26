use super::ServerOps;
use crate::{
    domain::ApiError,
    services::{design, design_systems, fixes, problems, selections, topics},
};
use async_trait::async_trait;
use rupicola_api::{
    CaptureSummary, Comment, DesignEntry, DesignLogOps, DesignSystemReadOps, DesignSystemWriteOps,
    DocumentEdit, Fix, FixReadOps, FixSelectionLink, FixSelectionOps, FixSummary, FixWriteOps,
    NewComment, NewDesignEntry, NewFix, NewProblem, NewReview, NewSelection, NewStep, NewTopic,
    Problem, ProblemPatch, ProblemReadOps, ProblemSelectionOps, ProblemSummary, ProblemWriteOps,
    Selection, SelectionOps, Step, SystemActionForm, SystemList, SystemVersion, Topic, TopicMove,
    TopicReadOps, TopicRename, TopicTree, TopicWriteOps,
};

#[async_trait]
impl ProblemReadOps for ServerOps {
    async fn problem_list(&self, project: &str) -> Result<Vec<ProblemSummary>, ApiError> {
        problems::list(&self.state, &self.principal, project).await
    }

    async fn problem_list_detailed(&self, project: &str) -> Result<Vec<Problem>, ApiError> {
        problems::list_details(&self.state, &self.principal, project).await
    }

    async fn problem_get(&self, id: &str) -> Result<Problem, ApiError> {
        problems::get(&self.state, &self.principal, id).await
    }
}

#[async_trait]
impl ProblemWriteOps for ServerOps {
    async fn problem_create(&self, project: &str, form: &NewProblem) -> Result<Problem, ApiError> {
        problems::create(&self.state, &self.principal, project, form).await
    }

    async fn problem_delete(&self, id: &str) -> Result<(), ApiError> {
        problems::delete(&self.state, &self.principal, id).await
    }

    async fn problem_triage(&self, id: &str, patch: &ProblemPatch) -> Result<Problem, ApiError> {
        problems::update(&self.state, &self.principal, id, patch).await
    }

    async fn problem_comment(&self, id: &str, form: &NewComment) -> Result<Comment, ApiError> {
        problems::comment(&self.state, &self.principal, id, form).await
    }

    async fn problem_analyze(
        &self,
        project: &str,
        capture: &str,
    ) -> Result<Vec<Problem>, ApiError> {
        problems::analyze_capture(&self.state, &self.principal, project, capture).await
    }
}

#[async_trait]
impl ProblemSelectionOps for ServerOps {
    async fn problem_cite_selection(&self, id: &str, selection: &str) -> Result<Problem, ApiError> {
        problems::cite_selection(&self.state, &self.principal, id, selection).await
    }

    async fn problem_uncite_selection(
        &self,
        id: &str,
        selection: &str,
    ) -> Result<Problem, ApiError> {
        problems::uncite_selection(&self.state, &self.principal, id, selection).await
    }
}

#[async_trait]
impl FixReadOps for ServerOps {
    async fn fix_list(&self, project: &str) -> Result<Vec<FixSummary>, ApiError> {
        fixes::list(&self.state, &self.principal, project).await
    }

    async fn fix_list_detailed(&self, project: &str) -> Result<Vec<Fix>, ApiError> {
        fixes::list_details(&self.state, &self.principal, project).await
    }

    async fn fix_get(&self, id: &str) -> Result<Fix, ApiError> {
        fixes::get(&self.state, &self.principal, id).await
    }
}

#[async_trait]
impl FixWriteOps for ServerOps {
    async fn fix_create(&self, project: &str, form: &NewFix) -> Result<Fix, ApiError> {
        fixes::create(&self.state, &self.principal, project, form).await
    }

    async fn fix_step(&self, id: &str, form: &NewStep) -> Result<Step, ApiError> {
        fixes::add_step(&self.state, &self.principal, id, form.clone()).await
    }

    async fn fix_review(&self, id: &str, form: &NewReview) -> Result<Fix, ApiError> {
        fixes::review(&self.state, &self.principal, id, form).await
    }
}

#[async_trait]
impl FixSelectionOps for ServerOps {
    async fn fix_cite_selection(
        &self,
        id: &str,
        selection: &str,
        link: &FixSelectionLink,
    ) -> Result<Fix, ApiError> {
        fixes::cite_selection(&self.state, &self.principal, id, selection, link).await
    }

    async fn fix_uncite_selection(&self, id: &str, selection: &str) -> Result<Fix, ApiError> {
        fixes::uncite_selection(&self.state, &self.principal, id, selection).await
    }
}

#[async_trait]
impl SelectionOps for ServerOps {
    async fn selection_create(
        &self,
        project: &str,
        form: &NewSelection,
    ) -> Result<Selection, ApiError> {
        selections::create(&self.state, &self.principal, project, form).await
    }

    async fn selection_get(&self, id: &str) -> Result<Selection, ApiError> {
        selections::get(&self.state, &self.principal, id).await
    }
}

#[async_trait]
impl DesignLogOps for ServerOps {
    async fn design_list(&self, project: &str) -> Result<Vec<DesignEntry>, ApiError> {
        design::list(&self.state, &self.principal, project).await
    }

    async fn design_record(
        &self,
        project: &str,
        form: &NewDesignEntry,
    ) -> Result<DesignEntry, ApiError> {
        design::create(&self.state, &self.principal, project, form.clone()).await
    }
}

#[async_trait]
impl DesignSystemReadOps for ServerOps {
    async fn design_system_list(&self, project: &str) -> Result<SystemList, ApiError> {
        design_systems::list(&self.state, &self.principal, project).await
    }

    async fn design_system_get(
        &self,
        project: &str,
        version: &str,
    ) -> Result<SystemVersion, ApiError> {
        design_systems::get(&self.state, &self.principal, project, version).await
    }
}

#[async_trait]
impl DesignSystemWriteOps for ServerOps {
    async fn design_system_create(&self, project: &str) -> Result<SystemVersion, ApiError> {
        design_systems::create(&self.state, &self.principal, project).await
    }

    async fn design_system_edit(
        &self,
        project: &str,
        version: &str,
        edit: &DocumentEdit,
    ) -> Result<SystemVersion, ApiError> {
        design_systems::edit(&self.state, &self.principal, project, version, edit.clone()).await
    }

    async fn design_system_act(
        &self,
        project: &str,
        version: &str,
        form: &SystemActionForm,
    ) -> Result<SystemVersion, ApiError> {
        design_systems::act(&self.state, &self.principal, project, version, form).await
    }
}

#[async_trait]
impl TopicReadOps for ServerOps {
    async fn topic_tree(&self, project: &str) -> Result<TopicTree, ApiError> {
        topics::tree(&self.state, &self.principal, project).await
    }

    async fn topic_captures(
        &self,
        id: &str,
        direct: bool,
    ) -> Result<Vec<CaptureSummary>, ApiError> {
        topics::captures(&self.state, &self.principal, id, direct).await
    }
}

#[async_trait]
impl TopicWriteOps for ServerOps {
    async fn topic_create(&self, project: &str, form: &NewTopic) -> Result<Topic, ApiError> {
        topics::create(&self.state, &self.principal, project, form)
            .await
            .map(|(topic, _)| topic)
    }

    async fn topic_rename(&self, id: &str, form: &TopicRename) -> Result<Topic, ApiError> {
        topics::rename(
            &self.state,
            &self.principal,
            id,
            &form.name,
            form.generation,
        )
        .await
    }

    async fn topic_move(&self, id: &str, form: &TopicMove) -> Result<Topic, ApiError> {
        topics::move_to(&self.state, &self.principal, id, form).await
    }

    async fn topic_delete(&self, id: &str, recursive: bool) -> Result<(), ApiError> {
        topics::remove(&self.state, &self.principal, id, recursive).await
    }

    async fn topic_file_capture(&self, id: &str, capture: &str) -> Result<(), ApiError> {
        topics::file(&self.state, &self.principal, id, capture).await
    }

    async fn topic_unfile_capture(&self, id: &str, capture: &str) -> Result<(), ApiError> {
        topics::unfile(&self.state, &self.principal, id, capture).await
    }
}
