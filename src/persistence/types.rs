use chrono::DateTime;
use chrono::Utc;
use r2d2_sqlite::SqliteConnectionManager;
use typemap::Key;

/// A User is a human using the bot
#[derive(Debug, Clone)]
pub struct User {
    /// User unique ID, Telegram generated
    pub id: i64,
    /// User first name, mandatory from telegram API
    pub first_name: String,
    /// User last name, optional
    pub last_name: Option<String>,
    /// User username, optional
    pub username: Option<String>,
}

/// The Channel structure is used for identifying both
/// channels and (super)groups, since they have the very same attributes.
#[derive(Debug)]
pub struct Channel {
    /// Channel unique ID, Telegram generated.
    pub id: i64,
    /// User unique ID, the user that registered the channel to the bot.
    /// Almost always the channel creator.
    pub registered_by: i64,
    /// The invitation link for the channel. Can be the user-decided one
    /// or the private-link Telegram generated.
    pub link: String,
    /// Channel name
    pub name: String,
}

/// A reference to the user that's currently managing a channel.
#[derive(Debug)]
pub struct BeingManagedChannel {
    /// User unique ID
    pub chan: i64,
}

/// An invitation sent from source, to dest, for the chan.
#[derive(Debug)]
pub struct Invite {
    /// Invitation unique ID, locally generated
    pub id: i64,
    /// Whenever the invitation has been created
    pub date: DateTime<Utc>,
    /// The user who's inviting
    pub source: i64,
    /// The user who's being invited
    pub dest: i64,
    /// The channel dest user is being invited into
    pub chan: i64,
}

/// A referral based strategy contest
#[derive(Debug)]
pub struct Contest {
    /// Contest unique ID, locally generated
    pub id: i64,
    /// Contest name, unique and locallyu generated
    pub name: String,
    /// The prize the owner of the `chan` wants to give to the contest's winner
    pub prize: String,
    /// Contest end date and time. Invitations received after the end date
    /// won't generate an increase in the ranking.
    pub end: DateTime<Utc>,
    /// Whenever the contenst's owner decided to start the Contest
    pub started_at: Option<DateTime<Utc>>,
    /// True if the user decided to stop this contest.
    pub stopped: bool,
    /// The channel ID for this contest
    pub chan: i64,
}

/// Helper struct contaning a rank ID and a Contest
#[derive(Debug)]
pub struct RankContest {
    /// A user rank (position)
    pub rank: i64,
    /// The contest associated
    pub c: Contest,
}

/// Rank is like a row in a ranking table.
#[derive(Debug, Clone)]
pub struct Rank {
    /// The position in the chart
    pub rank: i64,
    /// Number of invitations sent by this user
    pub invites: i64,
    /// The user that is in `rank` position because it sent `invites` invitations
    pub user: User,
}

/// Unique type for a `typemap::Key` used to fetch from the telexde context
/// the `r2d2::Pool<SqliteConnectionManager>`
pub struct DBKey;
impl Key for DBKey {
    type Value = r2d2::Pool<SqliteConnectionManager>;
}

/// Uniqye type for a `typemap::Key` used to fetch from the telexide context
/// the bot name, without accessing in this way to the `env`.
pub struct NameKey;
impl Key for NameKey {
    type Value = String;
}
