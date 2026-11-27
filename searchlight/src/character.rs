//! The character screen's data: each stat the player has, its value and every contribution to
//! it. The screen only shows what the stat layer resolved; it never recomputes a stat.

use moho_ui_shell::{ItemList, ItemLists, ListId, ListRow};
use searchlight_rules::{Contribution, SourceLabel, Stat, Verb, explain};

use crate::inventory::load_header;
use crate::menu::MenuAction;
use crate::sim::SearchlightSim;

/// The stats the screen lists, top to bottom, with the title of each list.
const STATS: [(Stat, &str); 3] = [
    (Stat::CarryCapacity, "Carry capacity"),
    (Stat::MovementSpeed, "Movement speed"),
    (Stat::Stamina, "Stamina"),
];

/// The screen: one list per stat, led by its value, then one row per contribution in
/// resolution order. Escape is back.
pub fn character_lists(sim: &SearchlightSim) -> ItemLists<MenuAction> {
    let contributions = sim.stat_contributions();
    let lists = STATS
        .iter()
        .zip(0..)
        .map(|(&(stat, title), id)| {
            let mut labels = vec![value_line(sim, stat)];
            labels.extend(
                explain(stat, &contributions)
                    .iter()
                    .map(|contribution| contribution_line(stat, contribution)),
            );
            let rows = labels
                .into_iter()
                .zip(0..)
                .map(|(label, row)| ListRow {
                    label,
                    count: None,
                    tooltip: None,
                    item: MenuAction::Item(row),
                    actions: Vec::new(),
                })
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
fn value_line(sim: &SearchlightSim, stat: Stat) -> String {
    let value = sim.stat(stat);
    match stat {
        Stat::CarryCapacity => {
            let capacity = value;
            format!(
                "Carrying {}",
                load_header(sim.load_kg(), capacity, sim.load_thresholds()).text
            )
        }
        Stat::MovementSpeed => format!("Now ×{value:.2}"),
        Stat::Stamina => format!("Now {value:.1} s"),
    }
}

/// One contribution as a row, such as "Load: speed ×0.70".
fn contribution_line(stat: Stat, contribution: &Contribution) -> String {
    let source = match &contribution.source {
        SourceLabel::Equipment(item) => item.to_string(),
        SourceLabel::Load => "Load".to_owned(),
        SourceLabel::Stance => "Stance".to_owned(),
    };
    let noun = match stat {
        Stat::CarryCapacity => "capacity",
        Stat::MovementSpeed => "speed",
        Stat::Stamina => "stamina",
    };
    let change = match contribution.verb {
        Verb::Add(x) => format!("{x:+.2}"),
        Verb::Scale(x) => format!("×{x:.2}"),
        Verb::Cap(x) => format!("capped at {x:.2}"),
    };
    format!("{source}: {noun} {change}")
}
