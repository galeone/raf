# Telegram RaF \[Refer a Friend\]([@RefafBot](https://t.me/RefafBot))

Telegram Bot for creating referral-based contests for your channels.

Create contests, let your users share your channel, increase your audience, and give prizes to the winners!

---

## Introduction

The software is written in [rust](https://github.com/rust-lang/rust). Raf depends on [a fork of telexide](https://github.com/galeone/telexide), a rust library for making telegram bots. The fork makes the original library work and solves some issues.

The storage used is SQLite: RaF creates a `raf.db` file in its run path where it saves all the relationships between:

- Who owns the channels
- The contests created
- The invitations each participant generated
- The users who joined the channel through an invitation

The code is still in development. Right now there's an MVP that just works, but the code is not well modularized nor documented.

## Setup

1. Install raf (development version):

```bash
cargo install --path .
```

2. Create the run path and the environment file

```bash
mkdir $HOME/.raf

echo 'BOT_NAME="<your bot name>"' > $HOME/.raf/raf.env
echo 'TOKEN="<your bot token>"' >> $HOME/.raf/raf.env
```

3. Copy the systemd service file

```bash
sudo cp misc/systemd/raf@.service /lib/systemd/system/
```

4. Start and enable the service

```bash
sudo systemctl start raf@$USER.service
sudo systemctl enable raf@$USER.service
```

The `raf.db` (to backup or inspect) is in `$HOME/.raf/`.
