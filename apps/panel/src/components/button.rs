use topcoat::{
    Result,
    context::Cx,
    view::{Attributes, Child, StaticClass, View, class, component, view},
};

/// The visual style of a [`button`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    /// The primary-filled button for the main action.
    #[default]
    Primary,
}

impl ButtonVariant {
    fn classes(self) -> StaticClass {
        match self {
            Self::Primary => class!(
                "border-transparent bg-primary text-primary-foreground shadow-xs",
                "hover:bg-primary/90 active:bg-primary/80",
            ),
        }
    }
}

/// The size of a [`button`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonSize {
    /// The standard button size.
    #[default]
    Md,
}

impl ButtonSize {
    fn classes(self) -> StaticClass {
        match self {
            Self::Md => class!("h-9 gap-2 rounded-lg px-4"),
        }
    }
}

/// Classes shared by button variants and sizes. A border reserves the same space in
/// every variant.
const BASE: StaticClass = class!(
    "inline-flex shrink-0 items-center justify-center border",
    "text-sm font-medium whitespace-nowrap transition-colors outline-none select-none",
    "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
    "focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50",
);

/// A styled button.
#[component]
pub async fn button(
    cx: &Cx,
    #[default] variant: ButtonVariant,
    #[default] size: ButtonSize,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <button
            class=(class!(
                BASE,
                variant.classes(),
                size.classes(),
                attrs.remove("class"),
            ))
            (attrs)
        >
            (child)
        </button>
    })
}
