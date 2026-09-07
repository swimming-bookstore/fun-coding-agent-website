use leptos::prelude::*;

mod agent;
mod pages;
mod vsh;
use pages::Home;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}

fn theme_now() -> String {
    if let Some(w) = web_sys::window() {
        if let Ok(Some(s)) = w.local_storage() {
            if let Ok(Some(t)) = s.get_item("fun-theme") {
                if t == "dark" || t == "light" {
                    return t;
                }
            }
        }
    }
    "light".into()
}

fn apply_theme(theme: &str) {
    if let Some(w) = web_sys::window() {
        if let Some(doc) = w.document() {
            if let Some(el) = doc.document_element() {
                let _ = el.set_attribute("data-theme", theme);
            }
        }
        if let Ok(Some(s)) = w.local_storage() {
            let _ = s.set_item("fun-theme", theme);
        }
    }
}

#[component]
pub fn Mark() -> impl IntoView {
    view! {
        <svg class="mark" viewBox="0 0 16 16" aria-hidden="true" focusable="false">
            <path
                fill="currentColor"
                d="M3.2 7.2c0-.66.54-1.2 1.2-1.2h.7V4.5C5.1 2.35 6.85 1.05 9.5 1.05c1.85 0 3.2.7 4.05 1.8l-1.7 1.4c-.5-.6-1.25-.95-2.2-.95-1.45 0-2.35.95-2.35 2.4v1.3h4c.66 0 1.2.54 1.2 1.2s-.54 1.2-1.2 1.2H7.25v5.35c0 .66-.54 1.2-1.2 1.2s-1.2-.54-1.2-1.2V9.6h-.7c-.66 0-1.2-.54-1.2-1.2z"
            ></path>
        </svg>
    }
}

#[component]
fn App() -> impl IntoView {
    let theme = RwSignal::new(theme_now());
    Effect::new(move |_| apply_theme(&theme.get()));
    view! {
        <div class="shell">
            <Nav theme/>
            <main class="wrap">
                <Home/>
            </main>
            <SiteFooter/>
        </div>
    }
}

#[component]
fn Nav(theme: RwSignal<String>) -> impl IntoView {
    view! {
        <nav class="top">
            <a href="/" class="brand">
                <Mark/>
                <span>"fun coding agent"</span>
            </a>
            <div class="top-nav">
                <button class="ghost" type="button" on:click=move |_| {
                    theme.update(|t| *t = if t.as_str() == "dark" { "light" } else { "dark" }.into());
                }>{move || if theme.get() == "dark" { "Light" } else { "Dark" }}</button>
            </div>
        </nav>
    }
}

#[component]
fn SiteFooter() -> impl IntoView {
    view! {
        <footer>
            <span class="copy">"© 수영 서점 Swimming Bookstore"</span>
            <a href="https://github.com/swimming-bookstore/fun-coding-agent" rel="noopener">"GitHub"</a>
        </footer>
    }
}
