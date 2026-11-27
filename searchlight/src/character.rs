//! The character screen's data: each stat the player has, its value and every contribution to
//! it. The screen only shows what the stat layer resolved; it never recomputes a stat.

use std::iter::once;

use moho_ui_shell::{ItemList, ItemLists, ListId};
use searchlight_rules::{Contribution, SourceLabel, Stat, Verb, explain};

use crate::inventory::load_header;
use crate::menu::{MenuAction, item_row};
use crate::sim::{CarrySheet, SearchlightSim};

/// The stats the screen lists, top to bottom, with the title of each list and the noun its
/// contribution rows name it by.
const STATS: [(Stat, &str, &str); 3] = [
    (Stat::CarryCapacity, "Carry capacity", "capacity"),
    (Stat::MovementSpeed, "Movement speed", "speed"),
    (Stat::Stamina, "Stamina", "stamina"),
];

/// The screen: one list per stat, led by its value, then one row per contribution in
/// resolution order. Escape is back.
pub fn character_lists(sim: &SearchlightSim) -> ItemLists<MenuAction> {
    let sheet = sim.carry_sheet();
    let lists = STATS
        .iter()
        .zip(0..)
        .map(|(&(stat, title, noun), id)| {
            let contributions = explain(stat, &sheet.contributions);
            let rows = once(value_line(&sheet, stat))
                .chain(
                    contributions
                        .iter()
                        .map(|contribution| contribution_line(noun, contribution)),
                )
                .enumerate()
                .map(|(row, label)| item_row(row, label, None))
                .collect();
            ItemList {
                id: ListId(id),
                title: title.to_owned(),
                rows,
            }
        })
        .collect();
    ItemLists {
        lists,
        back: Some(MenuAction::Back),
    }
}

/// The first row of a stat's list: what it resolved to.
fn value_line(sheet: &CarrySheet, stat: Stat) -> String {
    let value = sheet.stat(stat);
    match stat {
        Stat::CarryCapacity => format!(
            "Carrying {}",
            load_header(sheet.load_kg, value, sheet.thresholds()).text
        ),
        Stat::MovementSpeed => format!("Now ×{value:.2}"),
        Stat::Stamina => format!("Now {value:.1} s"),
    }
}

/// One contribution to the stat called `noun` as a row, such as "Load: speed ×0.70".
fn contribution_line(noun: &str, contribution: &Contribution) -> String {
    let source = match &contribution.source {
        SourceLabel::Equipment(item) => item.to_string(),
        SourceLabel::Load => "Load".to_owned(),
        SourceLabel::Stance => "Stance".to_owned(),
    };
    let change = match contribution.verb {
        Verb::Add(x) => format!("{x:+.2}"),
        Verb::Scale(x) => format!("×{x:.2}"),
        Verb::Cap(x) => format!("capped at {x:.2}"),
    };
    format!("{source}: {noun} {change}")
}
