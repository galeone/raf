use log::error;
use telexide::{
    api::types::{AnswerCallbackQuery, DeleteMessage, SendMessage},
    model::{InlineKeyboardButton, InlineKeyboardMarkup, ParseMode, ReplyMarkup},
    prelude::*,
};

use crate::persistence::types::Channel;

pub async fn display_main_commands(ctx: &Context, sender_id: i64) {
    let text = escape_markdown(
        "What do you want to do?\n\
        /register - Register a channel to the bot\n\
        /list - List your registered groups/channels\n\
        /contest - Start/Manage the referral contest\n\
        /rank - Your rank in the challenges you joined\n",
        None,
    );
    let mut reply = SendMessage::new(sender_id, &text);
    reply.set_parse_mode(&ParseMode::MarkdownV2);
    let res = ctx.api.send_message(reply).await;
    if res.is_err() {
        let err = res.err().unwrap();
        error!("[help] {}", err);
    }
}

pub fn escape_markdown(text: &str, entity_type: Option<&str>) -> String {
    let mut pattern = r#"'_*[]()~`>#+-=|{}.!"#;
    if let Some(entity) = entity_type {
        pattern = match entity {
            "pre" | "code" => r#"\`"#,
            "text_link" => r#"\)"#,
            _ => pattern,
        };
    }
    let pattern = format!("([{}])", regex::escape(pattern));
    let re = regex::Regex::new(&pattern).unwrap();
    return re.replace_all(text, r#"\$1"#).to_string();
}

pub async fn delete_parent_message(ctx: &Context, chat_id: i64, parent_message: Option<i64>) {
    if let Some(parent_id) = parent_message {
        let res = ctx
            .api
            .delete_message(DeleteMessage::new(chat_id, parent_id))
            .await;

        if res.is_err() {
            let err = res.err().unwrap();
            error!("[delete parent message] {}", err);
        }
    }
}

pub async fn display_manage_menu(ctx: &Context, chat_id: i64, chan: &Channel) {
    let mut reply = SendMessage::new(
        chat_id,
        &escape_markdown(&format!("{}\n\nWhat do you want to do?", chan.name), None),
    );
    reply.set_parse_mode(&ParseMode::MarkdownV2);
    let chan_id = chan.id;
    let inline_keyboard = vec![
        vec![
            InlineKeyboardButton {
                text: "✍️ Create".to_owned(),
                // start, chan
                callback_data: Some(format!("create {}", chan_id)),
                callback_game: None,
                login_url: None,
                pay: None,
                switch_inline_query: None,
                switch_inline_query_current_chat: None,
                url: None,
            },
            InlineKeyboardButton {
                text: "❌ Delete".to_owned(),
                callback_data: Some(format!("delete {}", chan_id)),
                callback_game: None,
                login_url: None,
                pay: None,
                switch_inline_query: None,
                switch_inline_query_current_chat: None,
                url: None,
            },
        ],
        vec![
            InlineKeyboardButton {
                text: "▶️ Start".to_owned(),
                // start, chan
                callback_data: Some(format!("start {}", chan_id)),
                callback_game: None,
                login_url: None,
                pay: None,
                switch_inline_query: None,
                switch_inline_query_current_chat: None,
                url: None,
            },
            InlineKeyboardButton {
                text: "⏹ Stop".to_owned(),
                callback_data: Some(format!("stop {}", chan_id)),
                callback_game: None,
                login_url: None,
                pay: None,
                switch_inline_query: None,
                switch_inline_query_current_chat: None,
                url: None,
            },
        ],
        vec![
            InlineKeyboardButton {
                text: "📄List".to_owned(),
                callback_data: Some(format!("list {}", chan_id)),
                callback_game: None,
                login_url: None,
                pay: None,
                switch_inline_query: None,
                switch_inline_query_current_chat: None,
                url: None,
            },
            InlineKeyboardButton {
                text: "🔙Menu".to_owned(),
                callback_data: Some(format!("main {}", chan_id)),
                callback_game: None,
                login_url: None,
                pay: None,
                switch_inline_query: None,
                switch_inline_query_current_chat: None,
                url: None,
            },
        ],
    ];
    reply.set_parse_mode(&ParseMode::MarkdownV2);
    reply.set_reply_markup(&ReplyMarkup::InlineKeyboardMarkup(InlineKeyboardMarkup {
        inline_keyboard,
    }));

    let res = ctx.api.send_message(reply).await;
    if res.is_err() {
        let err = res.err().unwrap();
        error!("[manage send] {}", err);
    }
}

pub async fn remove_loading_icon(ctx: &Context, callback_id: &str, text: Option<&str>) {
    let res = ctx
        .api
        .answer_callback_query(AnswerCallbackQuery {
            callback_query_id: callback_id.to_string(),
            cache_time: None,
            show_alert: text.is_some(),
            text: if text.is_some() {
                Some(text.unwrap().to_string())
            } else {
                None
            },
            url: None,
        })
        .await;
    if res.is_err() {
        error!("[remove_loading_icon] {}", res.err().unwrap());
    }
}
