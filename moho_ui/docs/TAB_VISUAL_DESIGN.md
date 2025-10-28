# Tab System - Visual Design

## Tab Bar Layout

```
┌─────────────────────────────────────────────────────────────────┐
│                         Game Settings                            │
│                                                                   │
│  ╔═══════════╗ ┌──────────┐ ┌──────────┐                        │
│  ║ Controls  ║ │  Audio   │ │ Graphics │                        │
│  ╚═══════════╝ └──────────┘ └──────────┘                        │
│──────────────────────────────────────────────────────────────────│
│                                                                   │
│  [Tab Content Scrollable Area]                                   │
│                                                                   │
│  Controls Section                                                │
│  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━                         │
│                                                                   │
│  Move Forward:                                        [W]        │
│  Move Left:                                           [A]        │
│  ...                                                              │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
  [Save Changes]  [Cancel] / [Back]
```

## Tab States

### Active Tab
- **Background**: Slightly lighter (#3C3C3C)
- **Border**: Top/sides only, accent color
- **Text**: Bright white
- **Cursor**: Default

### Inactive Tab
- **Background**: Darker (#282828)
- **Border**: Subtle bottom separator
- **Text**: Muted gray (#AAAAAA)
- **Cursor**: Pointer (indicates clickable)

### Hover (Inactive Tab)
- **Background**: Medium (#323232)
- **Text**: Lighter gray (#CCCCCC)

## Dirty State Indicator

When tab has unsaved changes:
```
┌──────────────┐
│ Audio *      │  <- Asterisk indicates changes
└──────────────┘
```

Or with visual indicator:
```
┌──────────────┐
│ ● Audio      │  <- Dot indicator
└──────────────┘
```

## Responsive Behavior

### Desktop (Wide)
All tabs visible horizontally:
```
[ Controls ] [ Audio ] [ Graphics ] [ Gameplay ] [ Network ]
```

### Future: Mobile/Narrow (if needed)
Tabs could wrap or become dropdown:
```
[ Controls ▼ ]
  ↓ Audio
  ↓ Graphics
  ↓ Gameplay
```

## Code Structure Visualization

```
SettingsMenu
├── State (unified)
│   ├── active_tab: SettingsTab
│   ├── prefs: Prefs
│   ├── staged: Prefs
│   └── dirty_fields: HashSet<SettingsField>
│
├── render()
│   ├── TopBottomPanel::top
│   │   ├── Heading: "Game Settings"
│   │   └── FormControls::tab_bar()  <- Renders tab buttons
│   │
│   ├── TopBottomPanel::bottom
│   │   └── Save/Cancel/Back buttons (shared across all tabs)
│   │
│   └── CentralPanel
│       └── match active_tab
│           ├── Controls => controls_tab::render()
│           ├── Audio => audio_tab::render()
│           └── Graphics => graphics_tab::render()
│
└── Tab Render Functions (modular)
    ├── controls_tab::render(&mut self, ui)
    ├── audio_tab::render(&mut self, ui)
    └── graphics_tab::render(&mut self, ui)
```

## Interaction Flow

```
User clicks "Audio" tab
    ↓
FormControls::tab_bar() returns index 1
    ↓
self.active_tab = SettingsTab::Audio
    ↓
Central panel matches active_tab
    ↓
audio_tab::render() called
    ↓
Audio controls displayed
    ↓
User modifies "Music Volume"
    ↓
dirty_fields.insert(AudioMusic)
    ↓
Save button enabled (unified across all tabs)
    ↓
User clicks "Controls" tab
    ↓
Controls tab shows, dirty state preserved
    ↓
User clicks "Save Changes"
    ↓
All changes across all tabs saved
```

## Implementation Notes

### Tab Bar Spacing
- **Tab Padding**: 12px horizontal, 8px vertical
- **Tab Gap**: 4px between tabs
- **Bar Bottom Margin**: 12px (spacing before content)

### Tab Content Area
- **Gutters**: 30% on each side (responsive)
- **Top Margin**: 0px (tab bar provides spacing)
- **Bottom Margin**: Auto (scrollable)

### Z-Index / Layering
- Active tab appears "in front" (higher fill, border)
- Inactive tabs appear "behind" (darker, recessed)
- Content area connects seamlessly to active tab

### Accessibility
- Keyboard navigation: Arrow keys or Tab to move between tabs
- Screen readers: Proper ARIA labels on tab buttons
- High contrast mode: Ensure tab distinction is clear
