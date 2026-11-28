/// Identifies an equippable tool. A newtype rather than a bare `u32` so a
/// future equipment system (armor, other gear — see `Pawn::equipped_tool`'s
/// doc comment) has a distinct id space to grow into, separate from
/// `resource_id`'s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ToolId(pub u32);

/// The tool every pawn spawns with — a basic mining tool. Only one tool
/// exists today; this becomes one of several once tool acquisition/crafting
/// lands (out of scope for 3.1c).
pub const STARTING_TOOL: ToolId = ToolId(1);
