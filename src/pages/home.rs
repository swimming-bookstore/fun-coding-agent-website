use crate::agent::{
    download_zip, logged_in, logout, poll_login, proxy_ready, start_login, turn, ChatLine, DeviceStart, LineKind,
    Vfs,
};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::HtmlTextAreaElement;

const PLATS: &[(&str, &str)] = &[
    ("cli", "CLI"),
    ("gtk", "GTK 4"),
    ("macos", "macOS"),
    ("ios", "iOS"),
    ("install", "Install"),
];

#[component]
pub fn Home() -> impl IntoView {
    let clip = RwSignal::new("cli");
    let live = RwSignal::new(false);
    Effect::new(move |_| {
        spawn_local(async move {
            live.set(proxy_ready().await);
        });
    });
    view! {
        <HeroAgent live/>

        <nav class="plats" aria-label="apps">
            {PLATS.iter().copied().map(|(id, label)| {
                view! {
                    <button
                        class=move || if clip.get() == id { "plat on" } else { "plat" }
                        type="button"
                        on:click=move |_| clip.set(id)
                    >
                        <b>{label}</b>
                    </button>
                }
            }).collect_view()}
        </nav>

        {move || match clip.get() {
            "install" => view! {
                <div class="install-pane">
                    <pre class="code">"cargo install --git https://github.com/swimming-bookstore/fun-coding-agent --bin fun\nfun login\nfun"</pre>
                    <ul class="paths">
                        <li><span>"config.json"</span>" ~/.config/fun/config.json"</li>
                        <li><span>"auth.json"</span>" ~/.local/share/fun/auth.json"</li>
                        <li><span>"sessions"</span>" ~/.local/share/fun/sessions/"</li>
                    </ul>
                </div>
            }.into_any(),
            "ios" => view! {
                <div class="ios-shots">
                    <img class="ios-shot" src="demo/chat.png" alt="" />
                    <img class="ios-shot" src="demo/settings.png" alt="" />
                </div>
            }.into_any(),
            name => {
                let src = format!("demo/{name}.mp4");
                view! { <video class="clip" src=src controls autoplay loop playsinline></video> }.into_any()
            }
        }}

        <div class="shots">
            <Shot title="Stream" note="Grok replies live in the thread." working=true>
                <div class="bubble me">"fix the tests"</div>
                <div class="tool-run">"  ⏺ bash(cargo test)"</div>
                <div class="bubble ai streaming">"  12 passed, 1 flaky"</div>
            </Shot>
            <Shot title="Thinking" note="Reasoning sits in a dock, not the chat." working=true extra=view! {
                <div class="tui-think">
                    <div class="tui-label">"thinking"</div>
                    <div class="tui-think-body">"mtime ties — sort by id"</div>
                </div>
            }.into_any()>
                <div class="bubble me">"why does list_sessions flap?"</div>
            </Shot>
            <Shot title="Tools" note="read, write, edit, bash — in the workspace." working=true>
                <div class="tui-tools-sum">"  3 tools succeeded"</div>
                <div class="tool-run">"  ⏺ read(src/session.rs)"</div>
                <div class="tool-run">"  ⏺ edit(src/session.rs)"</div>
                <div class="tool-run">"  ⏺ bash(cargo test session)"</div>
            </Shot>
            <Shot title="Interrupt" note="Ctrl+Enter. Stop now, inject this line." working=true extra=view! {
                <div class="tui-interrupt"><span class="tui-int-label">"interrupt"</span>"  skip the flaky one"</div>
            }.into_any()>
                <div class="bubble me">"run the suite"</div>
                <div class="bubble ai">"  still on cargo test…"</div>
            </Shot>
            <Shot title="Send now" note="Steer. Runs after the current tool." working=true extra=view! {
                <div class="tui-box tui-steer">
                    <div class="tui-label">"send now"</div>
                    <div class="tui-line">"then pin the clock"</div>
                </div>
            }.into_any()>
                <div class="tool-run">"  ⏺ bash(cargo test)"</div>
            </Shot>
            <Shot title="Queue" note="Enter while working. Runs after the turn." working=true extra=view! {
                <div class="tui-box tui-queue">
                    <div class="tui-label">"queue"</div>
                    <div class="tui-qrow">
                        <span><span class="tui-qnum">"1. "</span>"then commit"</span>
                        <span class="tui-ctrls">
                            <span class="steer">"send now"</span>
                            "  "
                            <span class="edit">"edit"</span>
                            "  "
                            <span class="move off">"move up"</span>
                            "  "
                            <span class="move">"move down"</span>
                            "  "
                            <span class="drop">"cancel"</span>
                        </span>
                    </div>
                    <div class="tui-qrow">
                        <span><span class="tui-qnum">"2. "</span>"then open a PR"</span>
                        <span class="tui-ctrls">
                            <span class="steer">"send now"</span>
                            "  "
                            <span class="edit">"edit"</span>
                            "  "
                            <span class="move">"move up"</span>
                            "  "
                            <span class="move off">"move down"</span>
                            "  "
                            <span class="drop">"cancel"</span>
                        </span>
                    </div>
                </div>
            }.into_any()>
                <div class="bubble me">"fix the tests"</div>
            </Shot>
            <Shot title="Paste chip" note="A long paste collapses. The full blob is still sent." working=false composer=false extra=view! {
                <div class="tui-composer">
                    <span class="tui-prompt">"›"</span>
                    <span>"hi "</span>
                    <span class="tui-chip">"[paste 12 lines ×]"</span>
                    <span class="cursor"></span>
                </div>
            }.into_any()>
                <div class="bubble me">"look at this log"</div>
                <div class="bubble ai">"  race is in list_sessions."</div>
            </Shot>
            <Shot title="Usage" note="Status bar: tokens, cache, cost, model." working=false usage=true>
                <div class="bubble me">"ship it"</div>
                <div class="bubble ai">"  committed."</div>
            </Shot>
        </div>
    }
}

#[component]
fn Shot(
    title: &'static str,
    note: &'static str,
    working: bool,
    #[prop(optional)] usage: bool,
    #[prop(default = true)] composer: bool,
    #[prop(optional)] extra: Option<AnyView>,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="shot">
            <p class="shot-tip"><b>{title}</b><span>{note}</span></p>
            <div class="tui">
                <div class="tui-thread">{children()}</div>
                {extra}
                {composer.then(|| view! {
                    <div class="tui-composer"><span class="tui-prompt">"›"</span><span class="cursor"></span></div>
                })}
                <div class="tui-status">
                    {working.then(|| view! { <span class="tui-spin">"⠋ working"</span> })}
                    <span class="tui-ws">"~/project"</span>
                    {usage.then(|| view! {
                        <span>"input tokens 12.3k"</span>
                        <span class="ok">"output tokens 4.5k"</span>
                        <span class="tui-reason">"reasoning tokens 8.1k"</span>
                        <span class="tui-cache">"cache hit 80.0%"</span>
                        <span class="tui-cost">"$0.012"</span>
                        <span>"12.3%/500k"</span>
                    })}
                    <span class="tui-right">
                        <span class="tui-model">"grok-4.6"</span>
                        <span>"medium"</span>
                    </span>
                </div>
            </div>
        </div>
    }
}

#[component]
fn HeroAgent(live: RwSignal<bool>) -> impl IntoView {
    let vfs = Rc::new(RefCell::new(Vfs::default()));
    let history = Rc::new(RefCell::new(Vec::<Value>::new()));
    let login_gen = RwSignal::new(0u64);
    let lines = RwSignal::new(Vec::<ChatLine>::new());
    let draft = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let authed = RwSignal::new(logged_in());
    let login = RwSignal::new(None::<DeviceStart>);
    let flash = RwSignal::new(String::new());
    let files = RwSignal::new(vfs.borrow().list());

    view! {
        <section class="hero">
            {move || (!live.get()).then(|| view! {
                <div class="hero-cover">
                    <h1>"A fun coding agent"</h1>
                </div>
            })}
            <div class=move || if live.get() { "demo" } else { "demo off" }>
                <div class="demo-chrome">
                    <span class=move || if busy.get() || authed.get() { "demo-status on" } else { "demo-status" }>
                        {move || if busy.get() { "working" } else if authed.get() { "signed in" } else { "" }}
                    </span>
                    <div class="demo-acts">
                        {move || {
                            if authed.get() {
                                view! {
                                    <button class="ghost tiny" type="button" on:click=move |_| {
                                        login_gen.update(|g| *g = g.wrapping_add(1));
                                        logout();
                                        authed.set(false);
                                        login.set(None);
                                    }>"Log out of Grok"</button>
                                }.into_any()
                            } else {
                                let login_s = login;
                                let flash_s = flash;
                                view! {
                                    <button class="btn-ink tiny" type="button" disabled=move || busy.get() || !live.get() on:click=move |_| {
                                        if busy.get() || !live.get() { return; }
                                        busy.set(true);
                                        flash_s.set(String::new());
                                        let gen = login_gen.get().wrapping_add(1);
                                        login_gen.set(gen);
                                        spawn_local(async move {
                                            match start_login().await {
                                                Ok(start) => {
                                                    login_s.set(Some(start.clone()));
                                                    match poll_login(&start, gen, || login_gen.get_untracked()).await {
                                                        Ok(()) => {
                                                            authed.set(true);
                                                            login_s.set(None);
                                                            flash_s.set("Logged in.".into());
                                                        }
                                                        Err(e) => {
                                                            login_s.set(None);
                                                            if e != "login cancelled" {
                                                                flash_s.set(e);
                                                            }
                                                        }
                                                    }
                                                }
                                                Err(e) => flash_s.set(e),
                                            }
                                            busy.set(false);
                                        });
                                    }>"Log in to Grok"</button>
                                }.into_any()
                            }
                        }}
                        <button class="ghost tiny" type="button" disabled=move || busy.get() on:click={
                            let vfs = vfs.clone();
                            let history = history.clone();
                            move |_| {
                                if busy.get() { return; }
                                *vfs.borrow_mut() = Vfs::default();
                                history.borrow_mut().clear();
                                lines.set(Vec::new());
                                draft.set(String::new());
                                login.set(None);
                                files.set(vfs.borrow().list());
                                flash.set("New agent. Same Grok login, empty chat and folder.".into());
                            }
                        }>"New chat"</button>
                    </div>
                </div>
                {move || login.get().map(|l| {
                    view! {
                        <div class="inline-login">
                            <h3>"Log in to Grok"</h3>
                            <p>"Visit xAI and enter this code:"</p>
                            <div class="user-code">{l.user_code.clone()}</div>
                            <p><a href=l.uri.clone() target="_blank" rel="noopener">{l.uri.clone()}</a></p>
                            <a class="btn-ink tiny" href=l.open_url.clone() target="_blank" rel="noopener">"Open xAI"</a>
                            <button class="ghost tiny" type="button" on:click=move |_| {
                                login_gen.update(|g| *g = g.wrapping_add(1));
                                login.set(None);
                                busy.set(false);
                            }>"Cancel"</button>
                        </div>
                    }
                })}
                {move || {
                    let t = flash.get();
                    if t.is_empty() { None } else { Some(view! { <p class="flash">{t}</p> }) }
                }}
                <div class="demo-body">
                    <aside class="file-rail">
                        <h4>"files"</h4>
                        <div class="file-list">
                        {move || {
                            let list = files.get();
                            if list.is_empty() || list == "(empty)" {
                                return view! { <div class="file-chip">"(empty)"</div> }.into_any();
                            }
                            list.lines().map(|name| {
                                let name = name.to_string();
                                view! { <div class="file-chip">{name}</div> }
                            }).collect_view().into_any()
                        }}
                        </div>
                        <button class="ghost tiny file-zip" type="button" on:click={
                            let vfs = vfs.clone();
                            move |_| {
                                match download_zip(&vfs.borrow()) {
                                    Ok(()) => flash.set("Downloading fun-demo.zip".into()),
                                    Err(e) => flash.set(e),
                                }
                            }
                        }>"Download as zip"</button>
                    </aside>
                    <div class="thread" id="demo-thread">
                    {move || {
                        let items = lines.get();
                        if items.is_empty() {
                            return view! { <div class="empty">"A fun coding agent"</div> }.into_any();
                        }
                        items.into_iter().map(|item| {
                            let class = match item.kind {
                                LineKind::User => "bubble me",
                                LineKind::Agent => "bubble ai",
                                LineKind::Tool => "tool-run",
                                LineKind::Note => "bubble note",
                            };
                            view! { <div class=class>{item.text}</div> }
                        }).collect_view().into_any()
                    }}
                    </div>
                </div>
                <form class="composer" on:submit={
                    let vfs = vfs.clone();
                    let history = history.clone();
                    move |ev| {
                        ev.prevent_default();
                        let text = draft.get();
                        if text.trim().is_empty() || busy.get() || !live.get() {
                            return;
                        }
                        if !authed.get() {
                            flash.set("Log in to Grok first.".into());
                            return;
                        }
                        draft.set(String::new());
                        busy.set(true);
                        lines.update(|l| l.push(ChatLine { kind: LineKind::User, text: text.clone() }));
                        let vfs = vfs.clone();
                        let history = history.clone();
                        spawn_local(async move {
                            let mut extra = Vec::new();
                            turn(&mut vfs.borrow_mut(), &mut history.borrow_mut(), &text, &mut extra).await;
                            lines.update(|l| l.extend(extra));
                            files.set(vfs.borrow().list());
                            busy.set(false);
                        });
                    }
                }>
                    <div class="composer-box">
                        <textarea
                            prop:value=move || draft.get()
                            placeholder=""
                            on:input=move |ev| {
                                if let Some(el) = ev.target().and_then(|t| t.dyn_into::<HtmlTextAreaElement>().ok()) {
                                    draft.set(el.value());
                                }
                            }
                            on:keydown=move |ev| {
                                if ev.key() == "Enter" && !ev.shift_key() {
                                    ev.prevent_default();
                                    if let Some(form) = ev.target()
                                        .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok())
                                        .and_then(|el| el.closest("form").ok().flatten())
                                        .and_then(|el| el.dyn_into::<web_sys::HtmlFormElement>().ok())
                                    {
                                        let _ = form.request_submit();
                                    }
                                }
                            }
                        />
                    </div>
                    <button class="btn-ink" type="submit" disabled=move || busy.get() || !live.get()>
                        {move || if busy.get() { "Working" } else { "Send" }}
                    </button>
                </form>
            </div>
        </section>
    }
}
