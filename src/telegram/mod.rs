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

//! `RaF` telegram communication.
//!
//! This create is based upon a [fork (galeone/telexide) ](https://github.com/galeone/telexide/) of [telexide](https://github.com/CalliEve/telexide).
//! The fork made it possible to use the library, since it was not maintained anymore and it was
//! using a very old version of tokio.
//!
//! It uses the `raf::persistence` crate too, since every action executed remotely can have some
//! local effect on the `RaF` storage.
//!
//! # What's inside this crate?
//!
//! - `channels`: functions for working with channels, like registering the channels to `RaF` or
//! getting the channels info. Despite the name, also groups and supergroups are supported, even
//! though they are always considered channels. Under the hood, there's almost zero differences
//! from the `RaF` goal.
//! - `commands`: the commands available to the `RaF` users, like `/start`, `/rank`, `/contest`. See
//! `/help` for the complete list of commands.
//! - `contests`: function for creating and updating the contests. The complete contest workflow is
//! not here, but in the `handlers` crate - because of how Telegram (and Telexide) works.
//! - `handlers`: the handlers for callback events (buttons, user interactions) and user messages.
//! - `messages`: functions for managing the text messages, like sending the `RaF` menu, working with
//! markdown, ...
//! - `users`: functions for getting a specific users or all the users that are channel owners.

pub mod channels;
pub mod commands;
pub mod contests;
pub mod handlers;
pub mod messages;
pub mod users;
