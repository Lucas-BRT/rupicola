//! Design systems: versions of a project's tokens and guidance, validated, reviewed and adopted.

use crate::{
    domain::{
        ApiError, Principal,
        design_system::{MAX_NOTES, Stored, valid, validate},
    },
    repositories::{audit, design_systems},
    services::access::{require, require_reader},
    state::AppState,
};
use rupicola_api::{
    Document, DocumentEdit, Role, SystemAction, SystemActionForm, SystemList, SystemPhase,
    SystemVersion,
};

/// Lists all design system versions of `project` and which one is adopted.
pub async fn list(
    state: &AppState,
    principal: &Principal,
    project: &str,
) -> Result<SystemList, ApiError> {
    require_reader(state, principal, project).await?;
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
    require_reader(state, principal, project).await?;
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
    let actor = principal.actor();
    let mut tx = state.db.begin().await?;
    let active = design_systems::adopted(&mut *tx, project).await?;
    let document = match active.as_deref() {
        Some(id) => design_systems::read(&mut *tx, project, id).await?.document,
        None => Document::default(),
    };
    let draft = Stored::draft(active.clone(), actor.to_owned(), document);
    design_systems::insert(&mut *tx, project, &draft).await?;
    audit::record(
        &mut *tx,
        Some(project),
        actor,
        &format!("Created design system draft {}", draft.id),
    )
    .await?;
    tx.commit().await?;
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
    let mut tx = state.db.begin().await?;
    let mut version = design_systems::read(&mut *tx, project, id).await?;
    if version.phase != SystemPhase::Draft || version.generation != form.generation {
        return Err(ApiError::Conflict("only the latest draft can be edited"));
    }
    version.document = form.document;
    version.report = None;
    design_systems::save(&mut *tx, project, &version).await?;
    audit::record(
        &mut *tx,
        Some(project),
        principal.actor(),
        &format!("Edited design system draft {id}"),
    )
    .await?;
    let active = design_systems::adopted(&mut *tx, project).await?;
    let updated = design_systems::read(&mut *tx, project, id)
        .await?
        .dto(active.as_deref());
    tx.commit().await?;
    Ok(updated)
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
    let actor = principal.actor();
    let mut tx = state.db.begin().await?;
    let mut version = design_systems::read(&mut *tx, project, id).await?;
    if version.generation != form.generation {
        return Err(ApiError::Conflict("the version changed; reload it"));
    }
    let active = design_systems::adopted(&mut *tx, project).await?;
    let (result_id, event) = match form.action {
        SystemAction::Validate => {
            if version.phase != SystemPhase::Draft {
                return Err(ApiError::Conflict("only a draft can be validated"));
            }
            let mut report = validate(&version.document);
            report.baseline = active;
            version.report = Some(report);
            design_systems::save(&mut *tx, project, &version).await?;
            (version.id, format!("Validated design system {id}"))
        }
        SystemAction::Submit => {
            version.submit(active.as_deref())?;
            design_systems::save(&mut *tx, project, &version).await?;
            (version.id, format!("Submitted design system {id}"))
        }
        SystemAction::Variation => {
            let new = Stored::draft(Some(version.id), actor.to_owned(), version.document);
            design_systems::insert(&mut *tx, project, &new).await?;
            let new_id = new.id;
            (new_id, format!("Created design system variation from {id}"))
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
                version.note(actor, "adopt", "document", String::new());
                design_systems::adopt(&mut tx, project, &version).await?;
            }
            (version.id, format!("Adopted design system {id}"))
        }
        SystemAction::Comment | SystemAction::Approve | SystemAction::Changes => {
            version.review(actor, form)?;
            design_systems::save(&mut *tx, project, &version).await?;
            (version.id, format!("Reviewed design system {id}"))
        }
    };
    audit::record(&mut *tx, Some(project), actor, &event).await?;
    let active = design_systems::adopted(&mut *tx, project).await?;
    let updated = design_systems::read(&mut *tx, project, &result_id)
        .await?
        .dto(active.as_deref());
    tx.commit().await?;
    Ok(updated)
}
