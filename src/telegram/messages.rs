// Copyright 2021 Paolo Galeone <nessuno@nerdz.eu>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use log::error;
use telexide_fork::{
    api::types::{AnswerCallbackQuery, DeleteMessage, SendMessage},
    model::{InlineKeyboardButton, InlineKeyboardMarkup, ParseMode, ReplyMarkup},
    prelude::*,
};

use crate::persistence::types::Channel;

/// Sends to the `chat_id` the list of the commands.
/// Used to show a raw menu to the user after the execution of any command.
///
/// # Arguments
/// * `ctx` - Telexide context
/// * `chat_id` - The chat ID.
///
/// # Panics
/// Panics if Telegram returns a error.
pub async fn display_main_commands(ctx: &Context, chat_id: i64) {
    let text = escape_markdown(
        "What do you want to do?\n\
        /register - Register a channel/group to the bot\n\
        /list - List your registered groups/channels\n\
        /contest - Start/Manage the referral contest\n\
        /rank - Your rank in the challenges you joined\n",
        None,
    );
    let mut reply = SendMessage::new(chat_id, &text);
    reply.set_parse_mode(&ParseMode::MarkdownV2);
    let res = ctx.api.send_message(reply).await;
    if res.is_err() {
        let err = res.err().unwrap();
        error!("[help] {}", err);
    }
}

/// Escape the input `text` to support Telegram Markdown V2.
/// Depending on the `entity_type` changes the rules. If in `pre`,`code` or `text_link`
/// there's only a small subset of characters to escape, otherwise the default pattern escapes
/// almost every non ASCII character.
///
/// # Arguments
/// * `text`: The string slice containing the text to escape
/// * `entity_type`: Optional entity type (`pre`, `code` or `text_link`).
///
/// # Panics
/// It panics if the regex used for the escape fails to be built.
#[must_use]
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

/// Deletes a message with `message_id` from `chat_id`.
///
/// # Arguments
/// * `ctx` - Telexide context
/// * `chat_id` - The chat ID.
/// * `message_id` - The ID of the message to delete.
///
/// # Panics
/// Panics if Telegram returns a error.
pub async fn delete_message(ctx: &Context, chat_id: i64, message_id: i64) {
    let res = ctx
        .api
        .delete_message(DeleteMessage::new(chat_id, message_id))
        .await;

    if res.is_err() {
        let err = res.err().unwrap();
        error!("[delete parent message] {}", err);
    }
}

/// Display the manage menu (grid of buttons), to use when the user is
/// creating/managing contests.
///
/// # Arguments
/// * `ctx` - Telexide context
/// * `chat_id` - The chat ID.
/// * `chan` - The channel that's being managed.
///
/// # Panics
/// Panics if Telegram returns a error.
pub async fn display_manage_menu(ctx: &Context, chat_id: i64, chan: &Channel) {
    let mut reply = SendMessage::new(
        chat_id,
        &escape_markdown(&format!("{}\n\nWhat do you want to do?", chan.name), None),
    );
    reply.set_parse_mode(&ParseMode::MarkdownV2);
    let inline_keyboard = vec![
        vec![
            InlineKeyboardButton {
                text: "\u{270d}\u{fe0f} Create".to_owned(),
                // start, chan
                callback_data: Some(format!("create {}", chan.id)),
                callback_game: None,
                login_url: None,
                pay: None,
                switch_inline_query: None,
                switch_inline_query_current_chat: None,
                url: None,
            },
            InlineKeyboardButton {
                text: "\u{274c} Delete".to_owned(),
                callback_data: Some(format!("delete {}", chan.id)),
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
                text: "\u{25b6}\u{fe0f} Start".to_owned(),
                // start, chan
                callback_data: Some(format!("start {}", chan.id)),
                callback_game: None,
                login_url: None,
                pay: None,
                switch_inline_query: None,
                switch_inline_query_current_chat: None,
                url: None,
            },
            InlineKeyboardButton {
                text: "\u{23f9} Stop".to_owned(),
                callback_data: Some(format!("stop {}", chan.id)),
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
                text: "\u{1f4c4}List".to_owned(),
                callback_data: Some(format!("list {}", chan.id)),
                callback_game: None,
                login_url: None,
                pay: None,
                switch_inline_query: None,
                switch_inline_query_current_chat: None,
                url: None,
            },
            InlineKeyboardButton {
                text: "\u{1f519}Menu".to_owned(),
                callback_data: Some(format!("main {}", chan.id)),
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

/// Removes the loading icon added by telegram to the user-clicked button.
///
/// # Arguments
/// * `ctx` - Telexide contest
/// * `callback_id` - The ID that generated the loading icon to be added
/// * `text`  - Optional text to show, in an alert, if present.
///
/// # Panics
/// Panics of Telegram returns an error.
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
