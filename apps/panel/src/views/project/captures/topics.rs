use crate::components::{controls::query_url, form::csrf};
use rupicola_api::Topic;
use topcoat::{
    Result,
    context::Cx,
    view::{View, component, view},
};

/// Topic filter chips and inline topic creation form.
#[component]
pub(super) async fn topics_bar(
    cx: &Cx,
    base: &str,
    topics: &[Topic],
    active_topic: &str,
    subview: &str,
    health: &str,
    query: &str,
    sort: &str,
    can_create: bool,
    token: &str,
) -> Result<impl View> {
    let captures_url = format!("{base}/captures");
    let view_param = if subview == "all" { "" } else { subview };
    let sort_param = if sort == "newest" { "" } else { sort };
    let all_href = query_url(
        &captures_url,
        &[
            ("view", view_param),
            ("health", health),
            ("q", query),
            ("sort", sort_param),
        ],
    );
    Ok(view! {
        <div class="propose-toolbar" aria-label="Topics">
            <nav class="filters" aria-label="Filter by topic">
                <a
                    class=(if active_topic.is_empty() { "button primary" } else { "button" })
                    href=(all_href)
                    data-swap-target="captures-region"
                >
                    "All topics"
                </a>
                for t in topics {
                    let href = query_url(
                        &captures_url,
                        &[
                            ("view", view_param),
                            ("health", health),
                            ("topic", t.id.as_str()),
                            ("q", query),
                            ("sort", sort_param),
                        ],
                    );
                    <a
                        class=(if active_topic == t.id { "button primary" } else { "button" })
                        href=(href)
                        data-swap-target="captures-region"
                    >
                        (format!("{} ({})", t.path, t.captures))
                    </a>
                }
            </nav>
            if can_create {
                <form class="filters" method="post" action=(&captures_url)>
                    csrf(token: token)
                    <label class="sr-only" for="new-topic-path">"New topic path"</label>
                    <input id="new-topic-path" name="path" type="text" required="" maxlength="120" placeholder="auth/login">
                    <button class="button" type="submit">"New topic"</button>
                </form>
            }
        </div>
    })
}

/// Rename, move, or delete the currently selected topic.
#[component]
pub(super) async fn topic_manage_drawer(
    cx: &Cx,
    base: &str,
    active: &Topic,
    topics: &[Topic],
    token: &str,
) -> Result<impl View> {
    let captures_url = format!("{base}/captures");
    let gen_str = active.generation.to_string();
    Ok(view! {
        <details class="surface panel-padding">
            <summary>(format!("Manage topic \"{}\"", active.path))</summary>
            <div class="filters">
                <form class="filters" method="post" action=(&captures_url)>
                    csrf(token: token)
                    <input type="hidden" name="action" value="rename">
                    <input type="hidden" name="topic_id" value=(&active.id)>
                    <input type="hidden" name="generation" value=(&gen_str)>
                    <label class="sr-only" for="rename-topic">"New topic name"</label>
                    <input id="rename-topic" name="name" type="text" required="" maxlength="64" value=(&active.name)>
                    <button class="button" type="submit">"Rename"</button>
                </form>
                <form class="filters" method="post" action=(&captures_url)>
                    csrf(token: token)
                    <input type="hidden" name="action" value="move">
                    <input type="hidden" name="topic_id" value=(&active.id)>
                    <input type="hidden" name="generation" value=(&gen_str)>
                    <label class="sr-only" for="move-topic-parent">"Parent topic"</label>
                    <select id="move-topic-parent" name="parent_id">
                        <option value="">"Root (top level)"</option>
                        for t in topics {
                            if t.id != active.id {
                                <option value=(&t.id) if active.parent_id.as_deref() == Some(t.id.as_str()) { selected="" }>(&t.path)</option>
                            }
                        }
                    </select>
                    <button class="button" type="submit">"Move"</button>
                </form>
                <form class="filters" method="post" action=(&captures_url)>
                    csrf(token: token)
                    <input type="hidden" name="action" value="delete">
                    <input type="hidden" name="topic_id" value=(&active.id)>
                    <label class="checkbox-row" for="del-topic-rec">
                        <input id="del-topic-rec" type="checkbox" name="recursive" value="true">
                        <span>"Include subtopics"</span>
                    </label>
                    <button class="button" type="submit">"Delete topic"</button>
                </form>
            </div>
        </details>
    })
}
