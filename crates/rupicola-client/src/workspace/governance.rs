use super::WorkspaceClient;
use crate::{Api, Result};
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
impl<A: Api> ProblemReadOps for WorkspaceClient<A> {
    async fn problem_list(&self, project: &str) -> Result<Vec<ProblemSummary>> {
        self.api.problems(&self.token, project).await
    }

    async fn problem_list_detailed(&self, project: &str) -> Result<Vec<Problem>> {
        self.api.problems_detailed(&self.token, project).await
    }

    async fn problem_get(&self, id: &str) -> Result<Problem> {
        self.api.problem(&self.token, id).await
    }
}

#[async_trait]
impl<A: Api> ProblemWriteOps for WorkspaceClient<A> {
    async fn problem_create(&self, project: &str, form: &NewProblem) -> Result<Problem> {
        self.api.create_problem(&self.token, project, form).await
    }

    async fn problem_delete(&self, id: &str) -> Result<()> {
        self.api.delete_problem(&self.token, id).await
    }

    async fn problem_triage(&self, id: &str, patch: &ProblemPatch) -> Result<Problem> {
        self.api.update_problem(&self.token, id, patch).await
    }

    async fn problem_comment(&self, id: &str, form: &NewComment) -> Result<Comment> {
        self.api.comment_problem(&self.token, id, form).await
    }

    async fn problem_analyze(&self, project: &str, capture: &str) -> Result<Vec<Problem>> {
        self.api
            .analyze_capture(&self.token, project, capture)
            .await
    }
}

#[async_trait]
impl<A: Api> ProblemSelectionOps for WorkspaceClient<A> {
    async fn problem_cite_selection(&self, id: &str, selection: &str) -> Result<Problem> {
        self.api
            .cite_problem_selection(&self.token, id, selection)
            .await
    }

    async fn problem_uncite_selection(&self, id: &str, selection: &str) -> Result<Problem> {
        self.api
            .uncite_problem_selection(&self.token, id, selection)
            .await
    }
}

#[async_trait]
impl<A: Api> FixReadOps for WorkspaceClient<A> {
    async fn fix_list(&self, project: &str) -> Result<Vec<FixSummary>> {
        self.api.fixes(&self.token, project).await
    }

    async fn fix_list_detailed(&self, project: &str) -> Result<Vec<Fix>> {
        self.api.fixes_detailed(&self.token, project).await
    }

    async fn fix_get(&self, id: &str) -> Result<Fix> {
        self.api.fix(&self.token, id).await
    }
}

#[async_trait]
impl<A: Api> FixWriteOps for WorkspaceClient<A> {
    async fn fix_create(&self, project: &str, form: &NewFix) -> Result<Fix> {
        self.api.create_fix(&self.token, project, form).await
    }

    async fn fix_step(&self, id: &str, form: &NewStep) -> Result<Step> {
        self.api.add_step(&self.token, id, form).await
    }

    async fn fix_review(&self, id: &str, form: &NewReview) -> Result<Fix> {
        self.api.review_fix(&self.token, id, form).await
    }
}

#[async_trait]
impl<A: Api> FixSelectionOps for WorkspaceClient<A> {
    async fn fix_cite_selection(
        &self,
        id: &str,
        selection: &str,
        link: &FixSelectionLink,
    ) -> Result<Fix> {
        self.api
            .cite_fix_selection(&self.token, id, selection, link)
            .await
    }

    async fn fix_uncite_selection(&self, id: &str, selection: &str) -> Result<Fix> {
        self.api
            .uncite_fix_selection(&self.token, id, selection)
            .await
    }
}

#[async_trait]
impl<A: Api> SelectionOps for WorkspaceClient<A> {
    async fn selection_create(&self, project: &str, form: &NewSelection) -> Result<Selection> {
        self.api.create_selection(&self.token, project, form).await
    }

    async fn selection_get(&self, id: &str) -> Result<Selection> {
        self.api.selection(&self.token, id).await
    }
}

#[async_trait]
impl<A: Api> DesignLogOps for WorkspaceClient<A> {
    async fn design_list(&self, project: &str) -> Result<Vec<DesignEntry>> {
        self.api.design(&self.token, project).await
    }

    async fn design_record(&self, project: &str, form: &NewDesignEntry) -> Result<DesignEntry> {
        self.api.add_design(&self.token, project, form).await
    }
}

#[async_trait]
impl<A: Api> DesignSystemReadOps for WorkspaceClient<A> {
    async fn design_system_list(&self, project: &str) -> Result<SystemList> {
        self.api.design_systems(&self.token, project).await
    }

    async fn design_system_get(&self, project: &str, version: &str) -> Result<SystemVersion> {
        self.api.design_system(&self.token, project, version).await
    }
}

#[async_trait]
impl<A: Api> DesignSystemWriteOps for WorkspaceClient<A> {
    async fn design_system_create(&self, project: &str) -> Result<SystemVersion> {
        self.api.create_design_system(&self.token, project).await
    }

    async fn design_system_edit(
        &self,
        project: &str,
        version: &str,
        edit: &DocumentEdit,
    ) -> Result<SystemVersion> {
        self.api
            .edit_design_system(&self.token, project, version, edit)
            .await
    }

    async fn design_system_act(
        &self,
        project: &str,
        version: &str,
        form: &SystemActionForm,
    ) -> Result<SystemVersion> {
        self.api
            .act_on_design_system(&self.token, project, version, form)
            .await
    }
}

#[async_trait]
impl<A: Api> TopicReadOps for WorkspaceClient<A> {
    async fn topic_tree(&self, project: &str) -> Result<TopicTree> {
        self.api.topic_tree(&self.token, project).await
    }

    async fn topic_captures(&self, id: &str, direct: bool) -> Result<Vec<CaptureSummary>> {
        self.api.topic_captures(&self.token, id, direct).await
    }
}

#[async_trait]
impl<A: Api> TopicWriteOps for WorkspaceClient<A> {
    async fn topic_create(&self, project: &str, form: &NewTopic) -> Result<Topic> {
        self.api.create_topic(&self.token, project, form).await
    }

    async fn topic_rename(&self, id: &str, form: &TopicRename) -> Result<Topic> {
        self.api.rename_topic(&self.token, id, form).await
    }

    async fn topic_move(&self, id: &str, form: &TopicMove) -> Result<Topic> {
        self.api.move_topic(&self.token, id, form).await
    }

    async fn topic_delete(&self, id: &str, recursive: bool) -> Result<()> {
        self.api.delete_topic(&self.token, id, recursive).await
    }

    async fn topic_file_capture(&self, id: &str, capture: &str) -> Result<()> {
        self.api.file_topic_capture(&self.token, id, capture).await
    }

    async fn topic_unfile_capture(&self, id: &str, capture: &str) -> Result<()> {
        self.api
            .unfile_topic_capture(&self.token, id, capture)
            .await
    }
}
