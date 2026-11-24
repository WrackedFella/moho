//! Item lists a game describes as data: any number of scrollable lists of rows (label and count)
//! side by side, with a move action between them. The shell owns focus, keys, layout and drawing;
//! the game decides what a move does and answers with new data.

/// A screen of lists side by side, and the action Escape chooses.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemLists<A> {
    /// The lists, left to right; a list is named by its index here.
    pub lists: Vec<ItemList<A>>,
    /// What Escape chooses on this screen; `None` makes Escape do nothing.
    pub back: Option<A>,
}

/// One titled list of rows.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemList<A> {
    pub title: String,
    pub rows: Vec<ListRow<A>>,
}

/// One row of a list.
#[derive(Clone, Debug, PartialEq)]
pub struct ListRow<A> {
    pub label: String,
    /// Drawn right-aligned when `Some`.
    pub count: Option<u32>,
    /// The game's text shown beside the row while it is focused.
    pub tooltip: Option<String>,
    /// What the game is told when the row is moved.
    pub item: A,
}

#[cfg(test)]
mod tests {
    use moho_input::bindings::Input;
    use moho_input::key::Key;
    use proptest::prelude::{prop, prop_assert, proptest};

    use super::*;
    use crate::menu::{MenuEvent, MenuStack, TEXT_COLORS};
    use crate::text::{Anchor, ScreenText};

    const SIZE: [f32; 2] = [1280.0, 720.0];
    const COLOR: [u8; 4] = TEXT_COLORS[0];
    const FOCUS: [u8; 4] = TEXT_COLORS[1];

    fn row(label: &str, count: Option<u32>, tooltip: Option<&str>, item: u32) -> ListRow<u32> {
        ListRow {
            label: label.to_owned(),
            count,
            tooltip: tooltip.map(str::to_owned),
            item,
        }
    }

    fn list(title: &str, rows: Vec<ListRow<u32>>) -> ItemList<u32> {
        ItemList {
            title: title.to_owned(),
            rows,
        }
    }

    fn lists_of(lists: Vec<ItemList<u32>>) -> ItemLists<u32> {
        ItemLists { lists, back: None }
    }

    fn stack_of(lists: Vec<ItemList<u32>>) -> MenuStack<u32> {
        let mut stack = MenuStack::new();
        stack.push_lists(lists_of(lists));
        stack
    }

    /// Bare rows named `<prefix>00`, `<prefix>01`, ... with items `first`, `first + 1`, ...
    fn rows_named(prefix: &str, count: usize, first: u32) -> Vec<ListRow<u32>> {
        (0..count)
            .map(|i| row(&format!("{prefix}{i:02}"), None, None, first + i as u32))
            .collect()
    }

    fn press(stack: &mut MenuStack<u32>, key: Key) {
        stack.on_key(Input::Key(key), true, false);
    }

    fn presses(stack: &mut MenuStack<u32>, key: Key, times: usize) {
        for _ in 0..times {
            press(stack, key);
        }
    }

    /// The move requests since the last call, as `(item, from, to)`.
    fn moves(stack: &mut MenuStack<u32>) -> Vec<(u32, usize, usize)> {
        stack
            .take_events()
            .into_iter()
            .map(|event| match event {
                MenuEvent::MoveRequested { item, from, to } => (item, from, to),
                other => panic!("expected only move requests, got {other:?}"),
            })
            .collect()
    }

    /// The vertical centre of a text, whatever its anchor.
    fn y_of(text: &ScreenText) -> f32 {
        SIZE[1] / 2.0 + text.offset[1]
    }

    fn find<'a>(texts: &'a [ScreenText], text: &str) -> &'a ScreenText {
        texts
            .iter()
            .find(|t| t.text == text)
            .unwrap_or_else(|| panic!("no text {text:?} among {texts:?}"))
    }

    fn is_shown(texts: &[ScreenText], text: &str) -> bool {
        texts.iter().any(|t| t.text == text)
    }

    /// The labels drawn in the focus colour.
    fn focused(texts: &[ScreenText]) -> Vec<&str> {
        texts
            .iter()
            .filter(|t| t.color == FOCUS)
            .map(|t| t.text.as_str())
            .collect()
    }

    /// The centre of row `index` of any list: rows start at the top band, 32 points each.
    fn row_y(index: usize) -> f32 {
        56.0 + 32.0 * index as f32 + 16.0
    }

    /// The left edge of list `index` of `of` lists: 320 wide, 64 apart, centred on the window.
    fn list_left(index: usize, of: usize) -> f32 {
        let total = 320.0 * of as f32 + 64.0 * (of as f32 - 1.0);
        SIZE[0] / 2.0 - total / 2.0 + 384.0 * index as f32
    }

    /// A point over the middle of row `row` of list `index` of `of` lists.
    fn over(index: usize, of: usize, row: usize) -> [f32; 2] {
        [list_left(index, of) + 160.0, row_y(row)]
    }

    #[test]
    fn a_row_draws_its_label_left_and_its_count_right() {
        let stack = stack_of(vec![
            list(
                "Carried",
                vec![
                    row("Bandage", Some(3), None, 1),
                    row("Rounds", Some(60), None, 2),
                    row("Note", None, None, 3),
                ],
            ),
            list("Container", vec![row("Rifle", Some(1), None, 4)]),
        ]);

        let texts = stack.texts(SIZE);

        let at = |anchor, x: f32, row: usize, color| (anchor, [x, row_y(row) - 360.0], 18.0, color);
        let shape = |text: &str| {
            let t = find(&texts, text);
            (t.anchor, t.offset, t.size, t.color)
        };
        assert_eq!(shape("Bandage"), at(Anchor::Left, 288.0, 0, FOCUS));
        assert_eq!(shape("3"), at(Anchor::Right, 608.0 - 1280.0, 0, FOCUS));
        assert_eq!(shape("Rounds"), at(Anchor::Left, 288.0, 1, COLOR));
        assert_eq!(shape("60"), at(Anchor::Right, 608.0 - 1280.0, 1, COLOR));
        assert_eq!(shape("Note"), at(Anchor::Left, 288.0, 2, COLOR));
        let on_note_row = texts.iter().filter(|t| y_of(t) == row_y(2)).count();
        assert_eq!(on_note_row, 1, "a row without a count draws only its label");
        assert_eq!(shape("Rifle"), at(Anchor::Left, 672.0, 0, COLOR));
        assert_eq!(shape("1"), at(Anchor::Right, 992.0 - 1280.0, 0, COLOR));
    }

    #[test]
    fn each_title_is_centred_over_its_list_in_the_top_band() {
        let stack = stack_of(vec![
            list("Carried", rows_named("a", 1, 1)),
            list("Container", rows_named("b", 1, 2)),
        ]);

        let texts = stack.texts(SIZE);

        for (title, centre) in [("Carried", 448.0), ("Container", 832.0)] {
            let t = find(&texts, title);
            assert_eq!(t.anchor, Anchor::Center, "{title}");
            assert_eq!(SIZE[0] / 2.0 + t.offset[0], centre, "{title}");
            assert!((0.0..56.0).contains(&y_of(t)), "{title} at {}", y_of(t));
            assert_eq!(t.color, COLOR, "{title}");
        }
    }

    #[test]
    fn down_to_the_last_row_scrolls_it_into_view() {
        let mut stack = stack_of(vec![list("Carried", rows_named("Row ", 40, 0))]);

        presses(&mut stack, Key::ArrowDown, 18);
        let at_the_bottom_edge = stack.texts(SIZE);
        press(&mut stack, Key::ArrowDown);
        let one_past = stack.texts(SIZE);
        presses(&mut stack, Key::ArrowDown, 20);
        let last = stack.texts(SIZE);

        assert_eq!(y_of(find(&at_the_bottom_edge, "Row 00")), row_y(0));
        assert_eq!(y_of(find(&at_the_bottom_edge, "Row 18")), row_y(18));
        assert!(!is_shown(&one_past, "Row 00"));
        assert_eq!(y_of(find(&one_past, "Row 19")), row_y(18));
        assert_eq!(y_of(find(&last, "Row 39")), 648.0);
        assert_eq!(find(&last, "Row 39").color, FOCUS);
        assert!(!is_shown(&last, "Row 00"));
        assert!(!is_shown(&last, "Row 20"));
    }

    #[derive(Clone, Copy, Debug)]
    enum Move {
        Up,
        Down,
        Left,
        Right,
    }

    fn arrow(step: u8) -> (Move, Key) {
        match step {
            0 => (Move::Up, Key::ArrowUp),
            1 => (Move::Down, Key::ArrowDown),
            2 => (Move::Left, Key::ArrowLeft),
            _ => (Move::Right, Key::ArrowRight),
        }
    }

    proptest! {
        #[test]
        fn the_focused_row_is_always_visible(
            counts in prop::collection::vec(0usize..=100, 1..=4),
            steps in prop::collection::vec(0u8..4, 0..80),
            height in 480.0f32..=1440.0,
        ) {
            let size = [1280.0, height];
            let lists: Vec<_> = counts
                .iter()
                .enumerate()
                .map(|(i, &n)| {
                    list(&format!("T{i}"), rows_named(&format!("L{i} R"), n, 1000 * i as u32))
                })
                .collect();
            let mut stack = stack_of(lists);
            let mut list_at = 0;
            let mut row_at = vec![0usize; counts.len()];
            let check = |stack: &MenuStack<u32>, list_at: usize, row_at: &[usize]| {
                if counts[list_at] == 0 {
                    return Ok(());
                }
                let label = format!("L{list_at} R{:02}", row_at[list_at]);
                let texts = stack.texts(size);
                let shown = texts.iter().find(|t| t.text == label);
                prop_assert!(shown.is_some(), "{label} is not drawn");
                let y = size[1] / 2.0 + shown.map_or(0.0, |t| t.offset[1]);
                prop_assert!(
                    (56.0 + 16.0..=size[1] - 56.0 - 16.0).contains(&y),
                    "{label} is centred at {y} in a window {} tall",
                    size[1]
                );
                Ok(())
            };
            check(&stack, list_at, &row_at)?;

            for step in steps {
                let (movement, key) = arrow(step);
                let rows = counts[list_at];
                match movement {
                    Move::Down if rows > 0 => row_at[list_at] = (row_at[list_at] + 1) % rows,
                    Move::Up if rows > 0 => {
                        row_at[list_at] = (row_at[list_at] + rows - 1) % rows;
                    }
                    Move::Left => list_at = (list_at + counts.len() - 1) % counts.len(),
                    Move::Right => list_at = (list_at + 1) % counts.len(),
                    Move::Up | Move::Down => {}
                }
                press(&mut stack, key);
                check(&stack, list_at, &row_at)?;
            }
        }
    }

    #[test]
    fn up_and_down_wrap_within_the_focused_list() {
        let mut stack = stack_of(vec![list("Carried", rows_named("a", 3, 10))]);

        press(&mut stack, Key::ArrowUp);
        let up_from_the_first = focused(&stack.texts(SIZE)).join(",");
        press(&mut stack, Key::ArrowDown);
        let down_from_the_last = focused(&stack.texts(SIZE)).join(",");
        stack.on_key(Input::Key(Key::ArrowDown), true, true);
        let a_repeat_still_moves = focused(&stack.texts(SIZE)).join(",");

        assert_eq!(up_from_the_first, "a02");
        assert_eq!(down_from_the_last, "a00");
        assert_eq!(a_repeat_still_moves, "a01");
    }

    #[test]
    fn enter_or_a_click_requests_a_move_and_changes_no_rows() {
        let data = lists_of(vec![
            list(
                "Carried",
                vec![
                    row("Bandage", Some(3), None, 10),
                    row("Rounds", Some(60), None, 11),
                ],
            ),
            list("Container", vec![row("Rifle", Some(1), None, 20)]),
        ]);
        let mut stack = MenuStack::new();
        stack.push_lists(data.clone());

        press(&mut stack, Key::Enter);
        let first = moves(&mut stack);
        press(&mut stack, Key::ArrowDown);
        press(&mut stack, Key::Enter);
        let second = moves(&mut stack);
        stack.on_key(Input::Key(Key::Enter), true, true);
        stack.on_key(Input::Key(Key::Enter), false, false);
        let repeat_and_release = moves(&mut stack);
        stack.click(over(1, 2, 0), SIZE);
        let click_on_the_container = moves(&mut stack);
        stack.click(over(0, 2, 1), SIZE);
        let click_on_the_second_row = moves(&mut stack);
        stack.click([640.0, row_y(0)], SIZE);
        stack.click(over(0, 2, 5), SIZE);
        stack.click([5.0, 5.0], SIZE);
        let misses = moves(&mut stack);

        assert_eq!(first, [(10, 0, 1)]);
        assert_eq!(second, [(11, 0, 1)]);
        assert_eq!(repeat_and_release, []);
        assert_eq!(click_on_the_container, [(20, 1, 0)]);
        assert_eq!(click_on_the_second_row, [(11, 0, 1)]);
        assert_eq!(misses, []);
        assert_eq!(stack.top_lists(), Some(&data), "the shell moves no row");
    }

    #[test]
    fn backspace_or_a_right_click_requests_a_move_to_the_previous_list() {
        let mut stack = stack_of(vec![
            list("Carried", rows_named("a", 2, 10)),
            list("Container", rows_named("b", 1, 20)),
        ]);

        press(&mut stack, Key::ArrowDown);
        press(&mut stack, Key::Backspace);
        let from_the_first = moves(&mut stack);
        stack.on_key(Input::Key(Key::Backspace), true, true);
        let repeat = moves(&mut stack);
        stack.right_click(over(1, 2, 0), SIZE);
        let right_click_on_the_container = moves(&mut stack);
        stack.right_click(over(0, 2, 0), SIZE);
        let right_click_on_the_carried = moves(&mut stack);
        stack.right_click([640.0, row_y(0)], SIZE);
        stack.right_click(over(0, 2, 5), SIZE);
        let misses = moves(&mut stack);

        assert_eq!(
            from_the_first,
            [(11, 0, 1)],
            "the first list wraps to the last"
        );
        assert_eq!(repeat, []);
        assert_eq!(right_click_on_the_container, [(20, 1, 0)]);
        assert_eq!(right_click_on_the_carried, [(10, 0, 1)]);
        assert_eq!(misses, []);
    }

    #[test]
    fn with_one_list_left_and_right_stay_and_a_move_names_it_twice() {
        let mut stack = stack_of(vec![list("Carried", rows_named("a", 2, 10))]);

        press(&mut stack, Key::ArrowRight);
        press(&mut stack, Key::ArrowLeft);
        press(&mut stack, Key::Enter);
        press(&mut stack, Key::Backspace);

        assert_eq!(moves(&mut stack), [(10, 0, 0), (10, 0, 0)]);
    }

    #[test]
    fn three_lists_lay_out_left_to_right_and_moves_name_source_and_destination() {
        let mut stack = stack_of(vec![
            list("Rig", vec![row("Magazine", Some(2), None, 1)]),
            list("Backpack", vec![row("Bandage", Some(3), None, 2)]),
            list("Body", vec![row("Pistol", Some(1), None, 3)]),
        ]);

        let texts = stack.texts(SIZE);
        let mut requested = Vec::new();
        for (list, expected) in [(0, 1), (1, 2), (2, 3)] {
            press(&mut stack, Key::Enter);
            press(&mut stack, Key::Backspace);
            requested.extend(moves(&mut stack));
            assert_eq!(expected, requested.last().map_or(0, |m| m.0), "list {list}");
            press(&mut stack, Key::ArrowRight);
        }
        stack.click(over(2, 3, 0), SIZE);
        stack.right_click(over(2, 3, 0), SIZE);
        requested.extend(moves(&mut stack));

        for (title, label, count, left) in [
            ("Rig", "Magazine", "2", 96.0),
            ("Backpack", "Bandage", "3", 480.0),
            ("Body", "Pistol", "1", 864.0),
        ] {
            let centre = left + 160.0;
            assert_eq!(find(&texts, title).anchor, Anchor::Center, "{title}");
            assert_eq!(
                SIZE[0] / 2.0 + find(&texts, title).offset[0],
                centre,
                "{title}"
            );
            assert_eq!(find(&texts, label).offset[0], left, "{label}");
            assert_eq!(
                find(&texts, count).offset[0],
                left + 320.0 - SIZE[0],
                "{count}"
            );
        }
        assert_eq!(
            requested,
            [
                (1, 0, 1),
                (1, 0, 2),
                (2, 1, 2),
                (2, 1, 0),
                (3, 2, 0),
                (3, 2, 1),
                (3, 2, 0),
                (3, 2, 1),
            ]
        );
    }

    #[test]
    fn left_and_right_switch_list_and_each_keeps_its_focus() {
        let mut stack = stack_of(vec![
            list("A", rows_named("a", 3, 10)),
            list("B", rows_named("b", 2, 20)),
        ]);
        let mut shown = vec![focused(&stack.texts(SIZE)).join(",")];
        let mut requested = Vec::new();

        press(&mut stack, Key::ArrowDown);
        shown.push(focused(&stack.texts(SIZE)).join(","));
        press(&mut stack, Key::ArrowRight);
        shown.push(focused(&stack.texts(SIZE)).join(","));
        press(&mut stack, Key::ArrowDown);
        shown.push(focused(&stack.texts(SIZE)).join(","));
        press(&mut stack, Key::Enter);
        requested.extend(moves(&mut stack));
        press(&mut stack, Key::ArrowRight);
        shown.push(focused(&stack.texts(SIZE)).join(","));
        press(&mut stack, Key::Enter);
        requested.extend(moves(&mut stack));
        press(&mut stack, Key::ArrowLeft);
        shown.push(focused(&stack.texts(SIZE)).join(","));
        press(&mut stack, Key::Enter);
        requested.extend(moves(&mut stack));

        assert_eq!(shown, ["a00", "a01", "b00", "b01", "a01", "b01"]);
        assert_eq!(requested, [(21, 1, 0), (11, 0, 1), (21, 1, 0)]);
    }

    #[test]
    fn only_the_focused_rows_tooltip_is_drawn_outside_its_list() {
        const TIPS: [&str; 4] = [
            "Clean cloth.\nStops bleeding.",
            "Loose rifle rounds.",
            "Semi-auto.\nNeeds rounds.",
            "Water.",
        ];
        let mut stack = stack_of(vec![
            list(
                "Carried",
                vec![
                    row("Bandage", Some(3), Some(TIPS[0]), 1),
                    row("Rounds", Some(60), Some(TIPS[1]), 2),
                    row("Note", None, None, 3),
                ],
            ),
            list(
                "Container",
                vec![
                    row("Rifle", Some(1), Some(TIPS[2]), 4),
                    row("Flask", Some(1), Some(TIPS[3]), 5),
                ],
            ),
        ]);
        let tips = |stack: &MenuStack<u32>| -> Vec<(String, Anchor, [f32; 2])> {
            stack
                .texts(SIZE)
                .into_iter()
                .filter(|t| TIPS.contains(&t.text.as_str()))
                .inspect(|t| assert!(TEXT_COLORS.contains(&t.color), "{t:?}"))
                .map(|t| (t.text, t.anchor, t.offset))
                .collect()
        };
        let left_of_first = |row: usize, tip: &str| {
            (
                tip.to_owned(),
                Anchor::Right,
                [288.0 - 16.0 - 1280.0, row_y(row) - 360.0],
            )
        };
        let right_of_last = |row: usize, tip: &str| {
            (
                tip.to_owned(),
                Anchor::Left,
                [992.0 + 16.0, row_y(row) - 360.0],
            )
        };

        let on_bandage = tips(&stack);
        press(&mut stack, Key::ArrowDown);
        let on_rounds = tips(&stack);
        press(&mut stack, Key::ArrowDown);
        let on_note = tips(&stack);
        press(&mut stack, Key::ArrowRight);
        let on_rifle = tips(&stack);
        press(&mut stack, Key::ArrowDown);
        let on_flask = tips(&stack);
        stack.hover(over(0, 2, 1), SIZE);
        let hovering_rounds = tips(&stack);

        assert_eq!(on_bandage, [left_of_first(0, TIPS[0])]);
        assert_eq!(on_rounds, [left_of_first(1, TIPS[1])]);
        assert_eq!(on_note, []);
        assert_eq!(on_rifle, [right_of_last(0, TIPS[2])]);
        assert_eq!(on_flask, [right_of_last(1, TIPS[3])]);
        assert_eq!(hovering_rounds, [left_of_first(1, TIPS[1])]);
    }

    #[test]
    fn a_middle_lists_tooltip_grows_right_from_its_own_edge() {
        let stack = {
            let mut stack = stack_of(vec![
                list("A", rows_named("a", 1, 1)),
                list("B", vec![row("b", None, Some("Middle"), 2)]),
                list("C", rows_named("c", 1, 3)),
            ]);
            press(&mut stack, Key::ArrowRight);
            stack
        };

        let texts = stack.texts(SIZE);

        let tip = find(&texts, "Middle");
        assert_eq!(tip.anchor, Anchor::Left);
        assert_eq!(tip.offset, [800.0 + 16.0, row_y(0) - 360.0]);
    }

    #[test]
    fn hover_focuses_the_row_and_list_under_the_cursor_and_a_miss_keeps_it() {
        let mut stack = stack_of(vec![
            list("A", rows_named("a", 3, 10)),
            list("B", rows_named("b", 3, 20)),
        ]);
        let mut requested = Vec::new();

        stack.hover(over(1, 2, 2), SIZE);
        press(&mut stack, Key::Enter);
        requested.extend(moves(&mut stack));
        stack.hover([640.0, row_y(0)], SIZE);
        stack.hover(over(1, 2, 3), SIZE);
        stack.hover([0.0, 0.0], SIZE);
        stack.hover([448.0, 20.0], SIZE);
        press(&mut stack, Key::Enter);
        requested.extend(moves(&mut stack));
        stack.hover(over(0, 2, 1), SIZE);
        press(&mut stack, Key::Enter);
        requested.extend(moves(&mut stack));

        assert_eq!(requested, [(22, 1, 0), (22, 1, 0), (11, 0, 1)]);
    }

    #[test]
    fn a_focused_empty_list_draws_no_tooltip_and_enter_requests_nothing() {
        let mut stack = stack_of(vec![
            list("Empty", Vec::new()),
            list(
                "Container",
                vec![row("Rifle", Some(1), Some("Semi-auto."), 4)],
            ),
        ]);

        let on_empty = stack.texts(SIZE);
        for key in [Key::Enter, Key::Backspace, Key::ArrowUp, Key::ArrowDown] {
            press(&mut stack, key);
        }
        stack.click(over(0, 2, 0), SIZE);
        stack.right_click(over(0, 2, 0), SIZE);
        stack.scroll(3.0);
        let after_keys = (moves(&mut stack), stack.texts(SIZE));
        press(&mut stack, Key::ArrowRight);
        let on_rifle = stack.texts(SIZE);
        press(&mut stack, Key::ArrowLeft);
        let back_on_empty = stack.texts(SIZE);

        assert!(!is_shown(&on_empty, "Semi-auto."));
        assert!(
            is_shown(&on_empty, "Empty"),
            "an empty list keeps its title"
        );
        assert_eq!(after_keys.0, []);
        assert!(!is_shown(&after_keys.1, "Semi-auto."));
        assert!(is_shown(&on_rifle, "Semi-auto."));
        assert!(!is_shown(&back_on_empty, "Semi-auto."));
        press(&mut stack, Key::Enter);
        assert_eq!(moves(&mut stack), []);
    }

    #[test]
    fn set_lists_keeps_the_list_and_clamps_focus_and_scroll() {
        let long = |count: usize| {
            lists_of(vec![
                list("A", rows_named("R", count, 100)),
                list("B", rows_named("S", 5, 200)),
            ])
        };
        let mut stack = MenuStack::new();
        stack.push_lists(long(40));

        stack.scroll(10.0);
        let scrolled = stack.texts(SIZE);
        stack.set_lists(long(40));
        let kept = stack.texts(SIZE);
        stack.set_lists(long(15));
        let clamped = stack.texts(SIZE);

        assert!(!is_shown(&scrolled, "R00"));
        assert_eq!(y_of(find(&scrolled, "R10")), row_y(0));
        assert_eq!(y_of(find(&kept, "R10")), row_y(0), "the scroll is kept");
        assert!(!is_shown(&kept, "R00"));
        assert_eq!(
            y_of(find(&clamped, "R00")),
            row_y(0),
            "the scroll is clamped"
        );
        assert_eq!(stack.top_lists(), Some(&long(15)));
    }

    #[test]
    fn set_lists_clamps_the_focused_row_and_the_focused_list_index() {
        let data = |a: usize, b: usize| {
            lists_of(vec![
                list("A", rows_named("a", a, 10)),
                list("B", rows_named("b", b, 20)),
            ])
        };
        let mut stack = MenuStack::new();
        stack.push_lists(data(5, 5));
        press(&mut stack, Key::ArrowRight);
        presses(&mut stack, Key::ArrowDown, 4);

        stack.set_lists(data(5, 2));
        let clamped_row = focused(&stack.texts(SIZE)).join(",");
        press(&mut stack, Key::Enter);
        let on_it = moves(&mut stack);
        press(&mut stack, Key::ArrowLeft);
        press(&mut stack, Key::Enter);
        let other_list_kept = moves(&mut stack);
        press(&mut stack, Key::ArrowRight);
        stack.set_lists(lists_of(vec![list("A", rows_named("a", 5, 10))]));
        let clamped_list = focused(&stack.texts(SIZE)).join(",");
        press(&mut stack, Key::Enter);
        let on_the_only_list = moves(&mut stack);

        assert_eq!(clamped_row, "b01");
        assert_eq!(on_it, [(21, 1, 0)]);
        assert_eq!(other_list_kept, [(10, 0, 1)]);
        assert_eq!(clamped_list, "a00");
        assert_eq!(on_the_only_list, [(10, 0, 0)]);
    }

    #[test]
    fn set_lists_keeps_a_long_lists_focused_row_in_view_after_it_shrinks() {
        let data = |count: usize| lists_of(vec![list("A", rows_named("R", count, 100))]);
        let mut stack = MenuStack::new();
        stack.push_lists(data(40));
        presses(&mut stack, Key::ArrowDown, 30);

        stack.set_lists(data(10));
        let texts = stack.texts(SIZE);

        assert_eq!(focused(&texts), ["R09"]);
        assert_eq!(y_of(find(&texts, "R09")), row_y(9));
    }

    #[test]
    fn scroll_moves_the_focused_list_and_keeps_the_remainder() {
        let mut stack = stack_of(vec![
            list("A", rows_named("R", 40, 100)),
            list("B", rows_named("S", 40, 200)),
        ]);

        stack.scroll(0.5);
        let half = stack.texts(SIZE);
        stack.scroll(0.5);
        let whole = stack.texts(SIZE);
        press(&mut stack, Key::ArrowRight);
        stack.scroll(3.0);
        let second_list = stack.texts(SIZE);
        stack.scroll(-100.0);
        let back_to_top = stack.texts(SIZE);
        stack.scroll(1000.0);
        let at_the_end = stack.texts(SIZE);

        assert_eq!(y_of(find(&half, "R00")), row_y(0));
        assert!(!is_shown(&whole, "R00"));
        assert_eq!(y_of(find(&whole, "R01")), row_y(0));
        assert_eq!(y_of(find(&whole, "S00")), row_y(0), "the other list stays");
        assert_eq!(y_of(find(&second_list, "S03")), row_y(0));
        assert_eq!(y_of(find(&second_list, "R01")), row_y(0));
        assert_eq!(y_of(find(&back_to_top, "S00")), row_y(0));
        assert_eq!(y_of(find(&at_the_end, "S21")), row_y(0));
        assert!(!is_shown(&at_the_end, "S20"));
        assert_eq!(y_of(find(&at_the_end, "S39")), 648.0);
    }
}
