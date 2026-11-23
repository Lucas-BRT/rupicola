use crate::components::button::button;
use topcoat::{
    context::Cx,
    view::{Child, attributes, view},
};

/// The sign-in and registration page.
#[derive(Clone, Copy)]
pub struct Sheet<'a> {
    pub title: &'a str,
    pub registering: bool,
    /// Whether this registration creates the workspace's first account.
    pub initial: bool,
    pub handle: &'a str,
    pub error: Option<&'a str>,
    /// The one-time token the form must send back.
    pub nonce: &'a str,
}

pub fn render<'a>(cx: &'a Cx, sheet: Sheet<'a>) -> Child<'a> {
    let Sheet {
        title,
        registering,
        initial,
        handle,
        error,
        nonce,
    } = sheet;
    Child::new(view! { cx =>
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>(title) " · Rupicola"</title>
                <link rel="stylesheet" href="/panel.css">
                <script src="/panel.js" defer=""></script>
            </head>
            <body class="auth-page">
                <div class="auth-brand">
                    <span class="mark" aria-hidden="true">"R"</span>
                    <strong>"Rupicola"</strong>
                </div>
                <main class="auth-layout">
                    <div class="auth-visual" aria-hidden="true">
                        <div class="viewfinder"><i></i><i></i><i></i><i></i><div class="aperture"></div></div>
                    </div>
                    <div class="auth-content">
                        <h1>(title)</h1>
                        if registering && initial {
                            <p class="muted">"Create the owner account for this workspace."</p>
                        }
                        if let Some(error) = error {
                            <p id="auth-error" class="notice error" role="alert" tabindex="-1" data-form-error="">(error)</p>
                        }
                        <form class="stack-form" method="post" action=(if registering { "/register" } else { "/login" })>
                            <input type="hidden" name="csrf" value=(nonce)>
                            <label for="handle">"Username"</label>
                            <input id="handle" name="handle" value=(handle) required="" maxlength="48" autocomplete="username"
                                autocapitalize="none" spellcheck="false" if error.is_some() { aria-describedby="auth-error" }>
                            <label for="password">"Password"</label>
                            <input id="password" name="password" required="" type="password" maxlength="1024"
                                autocomplete=(if registering { "new-password" } else { "current-password" })
                                if registering { minlength="12" aria-describedby="password-help" }>
                            if registering { <small id="password-help">"At least 12 characters."</small> }
                            if registering {
                                <label for="code">(if initial { "Setup code" } else { "Invitation code" })</label>
                                <input id="code" name="code" type="password" maxlength="128" autocomplete="off"
                                    if !initial { required="" }>
                                if initial { <small>"Needed unless the backend runs on this machine only."</small> }
                            }
                            button(attrs: attributes! { type="submit" class="mt-3 w-full" }, (if registering { "Create account" } else { "Sign in" }))
                        </form>
                        if !registering {
                            <details class="recovery"><summary>"Need to reset your password?"</summary>
                                <p class="muted">"Ask the workspace operator to reset your password from the panel host."</p>
                            </details>
                        }
                        <a href=(if registering { "/login" } else { "/register" })>
                            (if registering { "Return to sign in" } else { "Have an invitation?" })
                        </a>
                    </div>
                </main>
            </body>
        </html>
    })
}
