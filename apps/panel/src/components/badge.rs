use topcoat::{
    Result,
    context::Cx,
    view::{Attributes, Child, StaticClass, View, class, component, view},
};

/// The visual style of a [`badge`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeVariant {
    /// The primary-filled badge for highlighted statuses.
    #[default]
    Primary,
    /// A muted, tinted fill for neutral statuses.
    Secondary,
    /// A destructive-filled badge for errors and warnings.
    Destructive,
}

impl BadgeVariant {
    fn classes(self) -> StaticClass {
        match self {
            Self::Primary => class!("border-transparent bg-primary text-primary-foreground"),
            Self::Secondary => class!("border-transparent bg-foreground/5 text-foreground"),
            Self::Destructive => {
                class!("border-transparent bg-destructive text-destructive-foreground")
            }
        }
    }
}

/// Classes shared by badge variants. A border reserves the same space in every variant.
const BASE: StaticClass = class!(
    "inline-flex w-fit shrink-0 items-center justify-center gap-1 rounded-md",
    "border px-2 py-0.5 text-xs font-medium whitespace-nowrap",
);

/// A small label for a status or count.
#[component]
pub async fn badge(
    cx: &Cx,
    #[default] variant: BadgeVariant,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <span class=(class!(BASE, variant.classes(), attrs.remove("class"))) (attrs)>
            (child)
        </span>
    })
}
