//! Profile badges. Discord publishes a user's badges as flags; each one has an official picture on Discord's own
//! servers, which DBYB loads when needed (the pictures are not copied into this project).
//!
//! A bot can't read a user's Nitro subscription or boosts on other servers: Nitro is guessed from what only Nitro
//! allows (an animated picture, a banner), and the boost badge comes from how long the member has boosted *this* server.

#[derive(Clone)]
pub struct Badge {
    pub label: String,
    /// Address of the picture on Discord's servers, if known.
    url: Option<String>,
}

impl Badge {
    pub fn url(&self) -> Option<String> {
        self.url.clone()
    }
}

/// A badge picture served as adge-icons/<hash>.png.
fn badge(label: &str, hash: &str) -> Badge {
    Badge { label: label.to_string(), url: Some(format!("https://cdn.discordapp.com/badge-icons/{hash}.png")) }
}

/// A badge picture served as ssets/content/<hash>.svg (the newer ones, such as the Nitro levels).
fn asset(label: &str, hash: &str) -> Badge {
    Badge { label: label.to_string(), url: Some(format!("https://cdn.discordapp.com/assets/content/{hash}.svg")) }
}

/// The badges a user's public flags give, in the order Discord shows them.
pub fn from_flags(flags: u64) -> Vec<Badge> {
    let table: [(u64, Badge); 12] = [
        (1 << 0, badge("Discord Staff", "5e74e9b61934fc1f67c65515d1f7e60d")),
        (1 << 1, badge("Partnered Server Owner", "3f9748e53446a137a052f3454e2de41e")),
        (1 << 18, badge("Moderator Programs Alumni", "fee1624003e2fee35cb398e125dc479b")),
        (1 << 2, badge("HypeSquad Events", "bf01d1073931f921909045f3a39fd264")),
        (1 << 6, badge("HypeSquad Bravery", "8a88d63823d8a71cd5e390baa45efa02")),
        (1 << 7, badge("HypeSquad Brilliance", "011940fd013da3f7fb926e4a1cd2e618")),
        (1 << 8, badge("HypeSquad Balance", "3aa41de486fa12454c3761e8e223442e")),
        // the picture of the first Bug Hunter level is not known: it is shown as text
        (1 << 3, Badge { label: "Bug Hunter".to_string(), url: None }),
        (1 << 14, badge("Bug Hunter (gold)", "848f79194d4be5ff5f81505cbd0ce1e6")),
        (1 << 9, badge("Early Supporter", "7060786766c9c840eb3019e725d2b358")),
        (1 << 17, badge("Early Verified Bot Developer", "6df5892e0f35b051f8b61eace34f4967")),
        (1 << 22, badge("Active Developer", "6bdc42827a38498929a4920da12695d9")),
    ];
    table.into_iter().filter(|(bit, _)| flags & bit != 0).map(|(_, badge)| badge).collect()
}

/// The Nitro badge, for someone who uses something only Nitro allows (we can't see the subscription itself).
pub fn nitro() -> Badge {
    asset("Nitro (detected: animated picture or banner)", "f61ddf640d11d9bdfaf9de265eb862edf28986b659e545f293b54adf229433af")
}

/// The Nitro levels, with how long the subscription must have lasted (in months) to reach each one.
const NITRO_TIERS: [(&str, u32, &str); 9] = [
    ("Starter", 0, "f61ddf640d11d9bdfaf9de265eb862edf28986b659e545f293b54adf229433af"),
    ("Bronze", 1, "9226534d23ce85818ab0a768771c3083eaaa640c1ae5224b933945f0deefc310"),
    ("Silver", 3, "8b4b79fdb42105846a2f238bf24e64bc7ff7f1c8f5b1f23f33354eaf2be152b8"),
    ("Gold", 6, "8adb5e57de66f9e2bf9b764fb973a47fb3552466f0c32ed73b0f503c286abd98"),
    ("Platinum", 12, "79b503085598b92dba50758bca96744efd6255155eadf85a53f3b96e80cf1884"),
    ("Diamond", 24, "75f77bbb354870ee41d9dd0973994e4f143fe429e418695c05e1530902b61dd5"),
    ("Emerald", 36, "1619408cf7b7dba0a006701e4bc6e407e6b39de092c5b77f6f9f78e94de78bf4"),
    ("Ruby", 60, "21a7703be2d8407380960daa610d47299f4aeab889a1d1a615886771f669fa8a"),
    ("Opal", 72, "6aa7b974e34694c46a1cdfff1778d4f033a6eb4ea1a000bc22d11860d3ed3215"),
];

/// The Nitro badge a subscription of months months gives. A bot can't read that duration for other users,
/// so this is not used yet: it is here for the day Discord shows it (and to show your own level).
#[allow(dead_code)]
pub fn nitro_tier(months: u32) -> Badge {
    let (name, _, hash) = NITRO_TIERS.iter().rev().find(|(_, needed, _)| months >= *needed).copied().unwrap_or(NITRO_TIERS[0]);
    asset(&format!("Nitro {name}"), hash)
}

/// The boost badge for a member who has boosted this server for `months` months (it grows with time:
/// 1, 2, 3, 6, 9, 12, 15, 18 and 24 months). The 3 and 24 month levels have no known picture and show as text.
pub fn boost(months: u32) -> Badge {
    let tiers: [(u32, Option<&'static str>); 9] = [
        (1, Some("51040c70d4f20a921ad6674ff86fc95c")),
        (2, Some("0e4080d1d333bc7ad29ef6528b6f2fb7")),
        (3, None),
        (6, Some("df199d2050d3ed4ebf84d64ae83989f8")),
        (9, Some("996b3e870e8a22ce519b3a50e6bdd52f")),
        (12, Some("991c9f39ee33d7537d9f408c3e53141e")),
        (15, Some("cb3ae83c15e970e8f3d410bc62cb8b99")),
        (18, Some("7142225d31238f6387d9f09efaa02759")),
        (24, None),
    ];
    let (level, hash) = tiers.iter().rev().find(|(needed, _)| months >= *needed).copied().unwrap_or(tiers[0]);
    let label = if months < 1 { "Server Booster (new)".to_string() } else { format!("Server Booster: {months} month{} on this server (badge level {level})", if months == 1 { "" } else { "s" }) };
    Badge { label, url: hash.map(|hash| format!("https://cdn.discordapp.com/badge-icons/{hash}.png")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(flags: u64) -> Vec<String> {
        from_flags(flags).into_iter().map(|b| b.label).collect()
    }

    #[test]
    fn flags_give_badges_in_discords_order() {
        assert_eq!(labels((1 << 22) | (1 << 0) | (1 << 7)), ["Discord Staff", "HypeSquad Brilliance", "Active Developer"]);
        assert!(from_flags(0).is_empty());
        // flags that are not badges (verified bot, bot interactions...) give nothing
        assert!(from_flags((1 << 16) | (1 << 19)).is_empty());
    }

    #[test]
    fn badge_pictures_point_to_discords_servers() {
        let staff = &from_flags(1)[0];
        assert_eq!(staff.url().as_deref(), Some("https://cdn.discordapp.com/badge-icons/5e74e9b61934fc1f67c65515d1f7e60d.png"));
        assert!(from_flags(1 << 3)[0].url().is_none());
    }

    #[test]
    fn the_nitro_level_grows_with_the_subscription() {
        let name = |months| nitro_tier(months).label;
        assert_eq!(name(0), "Nitro Starter");
        assert_eq!(name(1), "Nitro Bronze");
        assert_eq!(name(5), "Nitro Silver");
        assert_eq!(name(12), "Nitro Platinum");
        assert_eq!(name(71), "Nitro Ruby");
        assert_eq!(name(72), "Nitro Opal");
        assert_eq!(name(500), "Nitro Opal");
        assert!(nitro_tier(30).url().is_some_and(|u| u.ends_with(".svg")));
    }

    #[test]
    fn the_boost_badge_grows_with_the_months() {
        let level = |months| boost(months).url();
        assert_eq!(level(0), level(1));
        assert_ne!(level(1), level(2));
        assert_eq!(level(4), None); // the 3 month level has no known picture
        assert_eq!(level(5), None);
        assert_eq!(level(7), level(6));
        assert!(level(18).is_some());
        assert_eq!(level(23), level(18));
        assert_eq!(level(24), None); // the 24 month level has no known picture
        assert_eq!(level(60), None);
        assert!(boost(14).label.contains("14 months"));
        assert!(boost(1).label.contains("1 month on"));
    }
}