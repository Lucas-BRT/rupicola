//! Reusable interactive controls: artistic custom dropdowns and segmented filters.

use topcoat::{
    Result,
    context::Cx,
    view::{View, component, view},
};

/// One selectable option in a [`select_menu`].
#[derive(Clone, Debug)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

impl SelectOption {
    #[must_use]
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

/// One tab/filter pill inside a [`segmented_control`].
#[derive(Clone, Debug)]
pub struct SegmentedOption {
    pub label: String,
    pub href: String,
    pub active: bool,
}

impl SegmentedOption {
    #[must_use]
    pub fn new(label: impl Into<String>, href: impl Into<String>, active: bool) -> Self {
        Self {
            label: label.into(),
            href: href.into(),
            active,
        }
    }
}

/// Builds a clean URL with only non-empty query parameters so filtered views are shareable.
#[must_use]
pub fn query_url(base: &str, params: &[(&str, &str)]) -> String {
    let mut query = String::new();
    for (key, value) in params {
        if value.is_empty() {
            continue;
        }
        if !query.is_empty() {
            query.push('&');
        }
        query.push_str(key);
        query.push('=');
        for byte in value.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    query.push(char::from(byte));
                }
                b' ' => query.push('+'),
                other => {
                    use std::fmt::Write as _;
                    let _ = write!(query, "%{other:02X}");
                }
            }
        }
    }
    if query.is_empty() {
        base.to_owned()
    } else {
        format!("{base}?{query}")
    }
}

/// Reusable segmented pill control. When `swap_target` is set, clicking an option
/// updates the URL and refreshes only `#<swap_target>` without replacing the whole page.
#[component]
pub async fn segmented_control(
    cx: &Cx,
    label: &str,
    options: &[SegmentedOption],
    #[default] swap_target: &str,
) -> Result<impl View> {
    let has_swap = !swap_target.is_empty();
    Ok(view! {
        <nav class="ui-segmented" aria-label=(label) data-segmented="">
            <span class="ui-segmented-indicator" aria-hidden="true"></span>
            for opt in options {
                <a href=(&opt.href)
                   if has_swap { data-swap-target=(swap_target) }
                   if opt.active { aria-current="page" }>(&opt.label)</a>
            }
        </nav>
    })
}

/// Dedicated custom-styled dropdown replacing browser-native `<select>` menus.
/// Works inside GET/POST forms via a hidden `<input>` and supports keyboard navigation,
/// auto-submission, and targeted partial-region swaps.
#[component]
pub async fn select_menu(
    cx: &Cx,
    #[into] id: String,
    name: &str,
    label: &str,
    selected: &str,
    options: &[SelectOption],
    #[default] auto_submit: bool,
    #[default] compact: bool,
    #[default] theme_choice: bool,
) -> Result<impl View> {
    let current_label = options
        .iter()
        .find(|o| o.value == selected)
        .or_else(|| options.first())
        .map_or("", |o| o.label.as_str());
    let trigger_id = format!("{id}-trigger");
    let menu_id = format!("{id}-menu");
    let root_class = if compact {
        "ui-dropdown is-compact"
    } else {
        "ui-dropdown"
    };
    Ok(view! {
        <div class=(root_class) data-dropdown="" if auto_submit { data-auto-submit="" }>
            <input type="hidden" id=(&id) name=(name) value=(selected)
                if theme_choice { data-theme-choice="" }>
            <button type="button"
                id=(&trigger_id)
                class="ui-dropdown-trigger"
                aria-haspopup="listbox"
                aria-expanded="false"
                aria-controls=(&menu_id)
                aria-label=(label)>
                <span class="ui-dropdown-value" data-dropdown-label="">(current_label)</span>
                <svg class="ui-dropdown-chevron" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                    <path d="m4 6 4 4 4-4"/>
                </svg>
            </button>
            <ul id=(&menu_id) class="ui-dropdown-menu" role="listbox" aria-labelledby=(&trigger_id) aria-hidden="true" inert="" tabindex="-1">
                for opt in options {
                    let is_selected = opt.value == selected;
                    <li role="presentation">
                        <button type="button"
                            class="ui-dropdown-option"
                            role="option"
                            data-dropdown-option=""
                            data-value=(&opt.value)
                            aria-selected=(if is_selected { "true" } else { "false" })>
                            <span>(&opt.label)</span>
                            <svg class="ui-dropdown-check" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                <path d="m3.5 8.5 3 3 6-6.5"/>
                            </svg>
                        </button>
                    </li>
                }
            </ul>
        </div>
    })
}
