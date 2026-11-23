//! Design systems: versions of a project's tokens and guidance, validated, reviewed and adopted.

use crate::{
    domain::{
        ApiError, Principal,
        design_system::{MAX_NOTES, Stored, valid, validate},
    },
    repositories::design_systems,
    services::access::access,
    state::AppState,
};
use rupicola_api::{
    Document, DocumentEdit, Role, SystemAction, SystemActionForm, SystemList, SystemPhase,
    SystemVersion,
};

fn author(principal: &Principal) -> String {
    principal
        .handle
        .clone()
        .unwrap_or_else(|| "device".to_owned())
}

async fn require(
    state: &AppState,
    principal: &Principal,
    project: &str,
    role: Role,
) -> Result<(), ApiError> {
    access(state, principal, project).await?.require(role)
}

/// Lists all design system versions of `project` and which one is adopted.
pub async fn list(
    state: &AppState,
    principal: &Principal,
    project: &str,
) -> Result<SystemList, ApiError> {
    require(state, principal, project, Role::Reader).await?;
    Ok(SystemList {
        items: design_systems::list(&state.db, project).await?,
        active: design_systems::adopted(&state.db, project).await?,
    })
}

/// Reads one design system version of `project`.
pub async fn get(
    state: &AppState,
    principal: &Principal,
    project: &str,
    id: &str,
) -> Result<SystemVersion, ApiError> {
    require(state, principal, project, Role::Reader).await?;
    let active = design_systems::adopted(&state.db, project).await?;
    Ok(design_systems::read(&state.db, project, id)
        .await?
        .dto(active.as_deref()))
}

/// Creates a new draft, starting from the adopted version when one exists.
pub async fn create(
    state: &AppState,
    principal: &Principal,
    project: &str,
) -> Result<SystemVersion, ApiError> {
    require(state, principal, project, Role::Owner).await?;
    let active = design_systems::adopted(&state.db, project).await?;
    let document = match active.as_deref() {
        Some(id) => design_systems::read(&state.db, project, id).await?.document,
        None => Document::default(),
    };
    let draft = Stored::draft(active.clone(), author(principal), document);
    design_systems::insert(&state.db, project, &draft).await?;
    Ok(draft.dto(active.as_deref()))
}

/// Updates the document of the latest draft version.
pub async fn edit(
    state: &AppState,
    principal: &Principal,
    project: &str,
    id: &str,
    form: DocumentEdit,
) -> Result<SystemVersion, ApiError> {
    require(state, principal, project, Role::Owner).await?;
    if !valid(&form.document) {
        return Err(ApiError::invalid(
            "the document is incomplete or contains invalid tokens",
        ));
    }
    let mut version = design_systems::read(&state.db, project, id).await?;
    if version.phase != SystemPhase::Draft || version.generation != form.generation {
        return Err(ApiError::Conflict("only the latest draft can be edited"));
    }
    version.document = form.document;
    version.report = None;
    design_systems::save(&state.db, project, &version).await?;
    let active = design_systems::adopted(&state.db, project).await?;
    Ok(design_systems::read(&state.db, project, id)
        .await?
        .dto(active.as_deref()))
}

/// Runs a lifecycle action on a design system version.
pub async fn act(
    state: &AppState,
    principal: &Principal,
    project: &str,
    id: &str,
    form: &SystemActionForm,
) -> Result<SystemVersion, ApiError> {
    require(state, principal, project, Role::Owner).await?;
    let actor = author(principal);
    let mut version = design_systems::read(&state.db, project, id).await?;
    if version.generation != form.generation {
        return Err(ApiError::Conflict("the version changed; reload it"));
    }
    let active = design_systems::adopted(&state.db, project).await?;
    let result_id = match form.action {
        SystemAction::Validate => {
            if version.phase != SystemPhase::Draft {
                return Err(ApiError::Conflict("only a draft can be validated"));
            }
            let mut report = validate(&version.document);
            if design_systems::adopted(&state.db, project)
                .await?
                .as_deref()
                != active.as_deref()
            {
                return Err(ApiError::Conflict(
                    "another system was adopted meanwhile; validate again",
                ));
            }
            report.baseline = active;
            version.report = Some(report);
            design_systems::save(&state.db, project, &version).await?;
            version.id
        }
        SystemAction::Submit => {
            version.submit(active.as_deref())?;
            design_systems::save(&state.db, project, &version).await?;
            version.id
        }
        SystemAction::Variation => {
            let new = Stored::draft(Some(version.id), actor, version.document);
            design_systems::insert(&state.db, project, &new).await?;
            new.id
        }
        SystemAction::Adopt => {
            if version.phase != SystemPhase::Approved {
                return Err(ApiError::Conflict(
                    "only an approved version can be adopted",
                ));
            }
            if active.as_deref() != Some(version.id.as_str()) {
                if version.notes.len() >= MAX_NOTES {
                    return Err(ApiError::invalid("the version has too many notes"));
                }
                version.note(&actor, "adopt", "document", String::new());
                design_systems::adopt(&state.db, project, &version).await?;
            }
            version.id
        }
        SystemAction::Comment | SystemAction::Approve | SystemAction::Changes => {
            version.review(&actor, form)?;
            design_systems::save(&state.db, project, &version).await?;
            version.id
        }
    };
    let active = design_systems::adopted(&state.db, project).await?;
    Ok(design_systems::read(&state.db, project, &result_id)
        .await?
        .dto(active.as_deref()))
}
