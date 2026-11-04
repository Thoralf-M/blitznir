//! The band: the selection until a chat is entered, then the transcript
//! (answers as markdown, the user's words literal, a Copy on every message).
//! It follows the text to the bottom while the reader is there (follow.rs).

use super::answer::{Body, use_selection};
use super::follow::use_follow;
use super::icons::{self, icon_button};
use super::opened::{self, Delete, DictationCard, Head};
use super::reasoning::{Live, Reasoning, use_live};
use super::section::choice;
use super::select::Selectable;
use super::style::{Look, Palette, head, use_palette};
use super::thinking::{Thinking, Wave, use_wave};
use super::thumbs::{Shown, thumbs};
use crate::panel::Panel;
use crate::panel::view::{self, Msg, RET, Ui};
use blitznir_core::markdown::select::{self, Piece};
use blitznir_core::store::Unthought;
use freya::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

/// "16:10" of a message's stamp.
fn clock(ts: &str) -> String {
    ts.get(11..16).unwrap_or_default().to_string()
}

#[derive(PartialEq)]
struct MessageCard {
    msg: Msg,
    /// Its place in the chat: what Send again sends.
    index: usize,
    /// Its first piece in the chat's selection.
    base: usize,
    /// The answer just in: the thinking wave fades out over it.
    wave: bool,
    /// Its trace's fold, when the band keeps it ([`Reasoning::fold`]).
    fold: Option<State<bool>>,
}

impl Component for MessageCard {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let panel = use_consume::<Arc<Panel>>();
        let shown = use_consume::<Shown>();
        let m = &self.msg;
        let user = m.role == "user";
        let (copy, text) = (panel.clone(), m.text.clone());
        let again_panel = panel.clone();
        let mut meta = rect()
            .horizontal()
            .cross_align(Alignment::Center)
            .spacing(6.0)
            .child(
                label()
                    .text(if user { "You" } else { "Assistant" })
                    .font_weight(FontWeight::SEMI_BOLD),
            )
            .child(label().text(clock(&m.ts)).font_size(11.0).color(p.muted))
            .child(icon_button(&p, icons::COPY, "Copy", true, move |_| {
                copy.copy(text.clone(), "Copied".into());
            }));
        if !user {
            let text = m.text.clone();
            let tip = "Write out: paste this answer into the app you came from";
            meta = meta.child(icon_button(&p, icons::OUT, tip, true, move |_| {
                panel.write_out(Some(text.clone()));
            }));
        }
        let body = Body {
            text: m.text.clone(),
            markdown: !user,
            base: self.base,
            muted: false,
        };
        let thinking = (!user && !m.thinking.is_empty()).then(|| Reasoning {
            text: m.thinking.clone(),
            unthought: m.unthought,
            fold: self.fold,
        });
        let again = self.index;
        let unanswered = m.unanswered.as_ref().map(|why| {
            let resend = again_panel.clone();
            // a narrow card puts the button under the note, whole
            rect()
                .width(Size::fill())
                .horizontal()
                .content(Content::wrap())
                .cross_align(Alignment::Center)
                .spacing(6.0)
                .child(label().text(why.says()).font_size(12.0).color(p.muted))
                .child(choice(&p, Look::Tool, "Send again", move |_| {
                    resend.resend(again)
                }))
        });
        rect()
            .width(Size::fill())
            .spacing(4.0)
            .child(meta)
            .maybe_child(thinking)
            .maybe_child(thumbs(&p, &m.images, None, shown))
            .child(body)
            .maybe_child(unanswered)
            .maybe_child(self.wave.then_some(Thinking { base: self.base }))
    }
}

fn note(p: &Palette, text: String) -> Label {
    label().text(text).color(p.muted)
}

fn selection(p: &Palette, ui: &Ui) -> Vec<Element> {
    let mut out: Vec<Element> = match &ui.selection {
        None => vec![note(p, "Copying the selection…".into()).into()],
        Some(s) if !s.is_empty() => {
            let where_ = if ui.editable == Some(false) {
                "Translate shows the result here"
            } else {
                "Translate replaces it"
            };
            let n = s.chars().count();
            vec![
                head(p, format!("Selected text · {n} characters · {where_}")).into(),
                Selectable::new()
                    .span(s.clone())
                    .width(Size::fill())
                    .in_band()
                    .into_element(),
            ]
        }
        Some(_) => vec![
            head(p, "Nothing selected").into(),
            note(
                p,
                format!("Type what you want below and press {RET}, add a screenshot or an image, or start a chat."),
            )
            .into(),
        ],
    };
    if !ui.preview.is_empty() {
        out.push(
            rect()
                .width(Size::fill())
                .height(Size::px(1.0))
                .background(p.line)
                .into(),
        );
        out.push(
            Selectable::new()
                .span(ui.preview.clone())
                .width(Size::fill())
                .in_band()
                .into_element(),
        );
    }
    out
}

/// The chat's cards, and its pieces of text in order for the selection;
/// `fading`: the thinking wave fades out over the last answer; `live`: the
/// streaming answer's fold, and the message it lands as.
fn chat(
    p: &Palette,
    ui: &Ui,
    wave: &Wave,
    (fading, live): (bool, Live),
) -> (Vec<Element>, Vec<Piece>) {
    let mut pieces = vec![];
    let mut out: Vec<Element> = vec![];
    // the row it is about first: its parts are the selection's first pieces
    let card = ui.about.as_ref().map(|item| {
        let card = DictationCard {
            item: item.clone(),
            deletable: view::viewing(ui),
            base: 0,
        };
        for part in opened::parts(item) {
            pieces.extend(select::pieces(&part.blocks()));
        }
        card
    });
    let n = ui.messages.len();
    for (i, m) in ui.messages.iter().enumerate() {
        let base = pieces.len();
        pieces.extend(select::pieces(&select::message(&m.text, m.role != "user")));
        let card = MessageCard {
            msg: m.clone(),
            index: i,
            base,
            wave: fading && i + 1 == n,
            fold: (live.1 == Some(i)).then_some(live.0),
        };
        out.push(rect().key(i).width(Size::fill()).child(card).into());
    }
    if let Some(card) = card {
        out.insert(
            0,
            rect()
                .key(usize::MAX)
                .width(Size::fill())
                .child(card)
                .into(),
        );
    }
    if !ui.messages.is_empty() {
        let n = ui.messages.len();
        let top = Head {
            title: ui.chat_name.clone(),
            note: format!("{n} message{}", if n == 1 { "" } else { "s" }),
            text: None,
            delete: Some(Delete {
                ask: "Delete this chat and its images?",
                tip: "Delete this chat and its images",
            }),
            rename: true,
        };
        let top = rect().key(usize::MAX - 1).width(Size::fill()).child(top);
        out.insert(0, top.into());
    }
    if view::viewing(ui) {
        out.push(
            note(
                p,
                format!("Ask about it below: {RET} starts a new chat about this text."),
            )
            .into(),
        );
    } else if ui.messages.is_empty() && ui.streaming.is_none() {
        let n = ui.selection.as_ref().map_or(0, |s| s.chars().count());
        let hint = if n > 0 {
            format!("Ask anything below. The selected text ({n} characters) goes along as context.")
        } else {
            "Ask anything below. A screenshot or a pasted image goes along with your message."
                .into()
        };
        out.push(head(p, "No messages yet").into());
        out.push(note(p, hint).into());
    }
    if let Some(text) = &ui.streaming {
        let wave = ui.thinking.then_some(wave);
        let trace = (!ui.trace.is_empty()).then(|| Reasoning {
            text: ui.trace.clone(),
            unthought: Unthought::No,
            fold: Some(live.0),
        });
        out.push(streaming(p, (text, trace), wave, &mut pieces));
    }
    (out, pieces)
}

/// The answer as it streams in, its pieces after the chat's; `wave`: the
/// thinking wave over it (it stands in for the dots before the first word).
fn streaming(
    p: &Palette,
    (text, trace): (&str, Option<Reasoning>),
    wave: Option<&Wave>,
    pieces: &mut Vec<Piece>,
) -> Element {
    let meta = label().text("Assistant").font_weight(FontWeight::SEMI_BOLD);
    let base = pieces.len();
    let body: Element = if text.is_empty() {
        note(p, if wave.is_some() { " " } else { "…" }.into()).into()
    } else {
        pieces.extend(select::pieces(&select::message(text, true)));
        Body {
            text: text.to_string(),
            markdown: true,
            base,
            muted: false,
        }
        .into_element()
    };
    let card = rect()
        .width(Size::fill())
        .spacing(4.0)
        .child(meta)
        .maybe_child(trace)
        .child(body);
    match wave {
        Some(w) => card
            .child(Thinking { base })
            .on_sized(w.card_sized())
            .into(),
        None => card.into(),
    }
}

#[derive(PartialEq)]
pub struct Band;

impl Component for Band {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let state = use_consume::<State<Ui>>();
        let ui = state.read();
        let follow = use_follow();
        let focus = use_a11y();
        let chosen = use_selection();
        // another chat, opened without a press (the tray, a hotkey): the
        // selection's offsets would pick out its text
        let (shown, switched) = (use_hook(|| Rc::new(RefCell::new(None))), chosen.clone());
        use_side_effect(move || {
            let u = state.read();
            let now = u.chatting.then(|| u.chat_id.clone());
            if shown.replace(now.clone()) != now {
                switched.clear();
            }
        });
        let (wave, fading) = use_wave(ui.thinking, ui.streaming.is_some(), ui.messages.len());
        let live = use_live(state, follow.clone());
        let panel = use_consume::<Arc<Panel>>();
        let open = ui.chatting.then(|| ui.chat_id.clone());
        let (items, pieces) = if ui.chatting {
            chat(&p, &ui, &wave, (fading && ui.thinking, live))
        } else {
            (selection(&p, &ui), vec![])
        };
        chosen.set_pieces(pieces);
        let (grown, framed, keys) = (follow.clone(), follow.clone(), follow.clone());
        let grown = move |e: Event<SizedEventData>| grown.content(e.area.height(), open.as_deref());
        let framed = move |e: Event<SizedEventData>| framed.viewport(e.area);
        let (c, down) = (chosen.clone(), chosen.clone());
        let keys = move |e: Event<KeyboardEventData>| {
            if c.key(&e, &panel) || keys.key(&e) {
                e.stop_propagation();
            }
        };
        // a press starts the chat's selection and focuses the transcript,
        // so ⌘C and the scroll keys reach it; the selection view's text is
        // Selectable, which keeps its press to itself
        let scroll = follow.controller();
        let (edge, dragged, let_go) = (follow.clone(), follow.clone(), follow);
        let click = move |e: Event<PointerEventData>| {
            down.down(&e);
            if e.data().is_primary() {
                edge.pressed(e.global_location().y as f32);
            }
            focus.request_focus();
        };
        let (moved, released, anywhere) = (chosen.clone(), chosen.clone(), chosen);
        ScrollView::new_controlled(scroll)
            .width(Size::fill())
            .height(Size::flex(1.0))
            .on_sized(framed)
            .child(
                rect()
                    .a11y_id(focus)
                    .a11y_focusable(true)
                    .width(Size::fill())
                    .padding((12.0, 16.0))
                    .spacing(if ui.chatting { 16.0 } else { 6.0 })
                    .on_sized(grown)
                    .on_key_down(keys)
                    .on_pointer_down(click)
                    .on_global_pointer_move(move |e: Event<PointerEventData>| {
                        dragged.dragged(e.global_location().y as f32);
                        moved.moved(&e);
                    })
                    .on_global_pointer_up(move |_| {
                        let_go.released();
                        released.released();
                    })
                    .on_global_pointer_down(move |_| anywhere.clear())
                    .children(items),
            )
    }
}
