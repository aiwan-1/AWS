# Rust Code Editor — Roadmap & Features

Status legend: `[x]` done · `[~]` partial · `[ ]` not yet

## Editing
- [x] Multi-line text editing
- [x] Monospace + syntax highlighting (syntect)
- [ ] Multi-cursor / column selection
- [ ] Find & replace (in file)
- [ ] Find in files (project-wide)
- [ ] Undo/redo history per buffer
- [ ] Auto-indent / smart indent
- [ ] Bracket matching & rainbow brackets
- [ ] Auto-close brackets/quotes
- [ ] Code folding
- [ ] Word wrap toggle
- [ ] Line numbers / gutter
- [ ] Minimap
- [ ] Whitespace & invisibles rendering
- [ ] Comment toggle (Ctrl+/)
- [ ] Move/duplicate line shortcuts
- [ ] Trim trailing whitespace on save
- [ ] EOL / encoding selection

## Files & Workspace
- [x] File explorer tree
- [x] Tabs with dirty indicator
- [x] Open file / open folder
- [x] Save / Save As
- [ ] Workspace / multi-root workspace
- [ ] Recent files & folders
- [ ] File watchers (auto-reload on external change)
- [ ] New file / new folder from explorer
- [ ] Rename / delete / move from explorer
- [ ] Drag-and-drop in explorer
- [ ] `.gitignore`-aware filtering
- [ ] Breadcrumbs bar
- [ ] Split editor (vertical/horizontal)
- [ ] Pinned tabs

## Navigation
- [ ] Command palette (Ctrl+Shift+P)
- [ ] Quick open file (Ctrl+P)
- [ ] Go to symbol in file (Ctrl+Shift+O)
- [ ] Go to symbol in workspace (Ctrl+T)
- [ ] Go to line (Ctrl+G)
- [ ] Go to definition / declaration / type / references
- [ ] Peek definition / references
- [ ] Back/forward navigation history
- [ ] Outline / symbols panel

## Code Intelligence (LSP)
- [ ] LSP client framework
- [ ] Autocomplete / IntelliSense
- [ ] Hover info / signature help
- [ ] Diagnostics (errors, warnings, hints)
- [ ] Quick fixes / code actions
- [ ] Refactoring (rename symbol, extract, etc.)
- [ ] Format document / format on save
- [ ] Snippets

## Search
- [ ] Find / replace in current file (with regex)
- [ ] Find in files panel
- [ ] Replace across files with preview
- [ ] Search history

## Git
- [x] Source Control panel (stage, unstage, commit)
- [x] Branch indicator in status bar
- [x] Diff view (unified, inline in panel)
- [x] Push / pull / fetch (via system git)
- [x] Discard changes
- [ ] Status indicators in gutter
- [ ] Branch switcher / create branch
- [ ] Merge conflict UI
- [ ] Blame / history

## Terminal & Tasks
- [ ] Integrated terminal (PTY)
- [ ] Multiple terminals / split
- [ ] Tasks (`tasks.json`-style runners)
- [ ] Build/run/test buttons
- [ ] Problems panel parsing compiler output

## Debugging
- [ ] Debug Adapter Protocol client
- [ ] Breakpoints (line / conditional / logpoints)
- [ ] Variables / watch / call stack panels
- [ ] Step in/out/over
- [ ] Run without debugging

## UI / UX
- [x] Menu bar
- [x] Status bar (cursor pos, language, branch)
- [x] Sidebar panels (Explorer / Source Control switcher)
- [ ] Activity bar (left rail icons)
- [ ] Bottom panel (terminal/problems/output)
- [ ] Command palette
- [ ] Notifications / toasts
- [ ] Keyboard shortcut customization
- [ ] Light/dark theme toggle
- [ ] Theme marketplace / custom themes
- [ ] Font/zoom controls
- [ ] Welcome / start page
- [ ] Settings UI + JSON

## Project / Build
- [ ] `cargo`, `npm`, etc. integration
- [ ] Run/debug configurations
- [ ] Test explorer
- [ ] Output channels per provider

## Extensibility
- [ ] Plugin/extension API
- [ ] Extension marketplace
- [ ] Plugin sandboxing
- [ ] Settings sync

## Collaboration & Remote
- [ ] Live Share-style co-editing
- [ ] Remote SSH / containers / WSL
- [ ] Open folder over SFTP

## Persistence
- [ ] Restore session (open files, layout) on launch
- [ ] Per-workspace settings
- [ ] Per-user settings
