//! Shared 1Password picker modal state enums.

use std::collections::HashSet;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

pub use crate::tui::components::list_helpers::matches_filter;
use crate::tui::components::spinner::SPINNER_FRAMES;
use jackin_tui::components::scrollable_panel::render_selected_lines_in_area;
use jackin_tui::components::{Panel, PanelFocus, TextInputState};
use jackin_tui::theme::{PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};

/// Browse-only vs. creation-enabled picker mode.
#[derive(Debug, Clone)]
pub enum OpPickerMode {
    /// Pick an existing field only.
    Browse,
    /// Enable item/field/section creation rows and naming sub-stages.
    Create {
        item_name_default: String,
        field_label_default: String,
    },
}

impl OpPickerMode {
    pub const fn is_create(&self) -> bool {
        matches!(self, Self::Create { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpPickerStage {
    Account,
    Vault,
    Item,
    Section,
    Field,
    NewItemName,
    FieldLabel,
    NewSectionName,
}

impl OpPickerStage {
    pub const fn is_naming(self) -> bool {
        matches!(
            self,
            Self::NewItemName | Self::FieldLabel | Self::NewSectionName
        )
    }

    pub const fn is_filterable(self) -> bool {
        matches!(self, Self::Account | Self::Vault | Self::Item | Self::Field)
    }
}

/// Which creation path entered the field-label sub-stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldLabelOrigin {
    NewItem,
    NewField,
    NewSection,
}

impl FieldLabelOrigin {
    pub const fn cancel_stage(self) -> OpPickerStage {
        match self {
            Self::NewItem => OpPickerStage::NewItemName,
            Self::NewField => OpPickerStage::Field,
            Self::NewSection => OpPickerStage::NewSectionName,
        }
    }
}

#[derive(Debug, Clone)]
pub enum OpLoadState {
    Idle,
    Loading { spinner_tick: u8 },
    Ready,
    Error(OpPickerError),
}

#[derive(Debug, Clone)]
pub enum OpPickerError {
    Fatal(OpPickerFatalState),
    Recoverable { message: String },
}

#[derive(Debug, Clone)]
pub enum OpPickerFatalState {
    NotInstalled,
    NotSignedIn,
    NoVaults,
    GenericFatal { message: String },
}

/// Background load completion routed back into the picker.
#[derive(Debug)]
pub enum OpPickerLoadResult<Account, Vault, Item, Field> {
    Accounts(anyhow::Result<Vec<Account>>),
    Vaults(anyhow::Result<Vec<Vault>>),
    Items(anyhow::Result<Vec<Item>>),
    Fields(anyhow::Result<Vec<Field>>),
}

/// Typed request for external 1Password metadata loading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpPickerLoadRequest {
    Accounts,
    Vaults {
        account_id: Option<String>,
    },
    Items {
        account_id: Option<String>,
        vault_id: String,
    },
    Fields {
        account_id: Option<String>,
        vault_id: String,
        item_id: String,
    },
}

/// What the operator chose when the picker commits.
#[derive(Debug, Clone)]
pub enum OpPickerSelection<Reference, Account, Vault, Item, FieldTarget> {
    /// An existing field was chosen.
    Existing(Reference),
    /// Create a brand-new item in the vault.
    NewItem {
        account: Option<Account>,
        vault: Vault,
        item_name: String,
        section: Option<String>,
        field_label: String,
    },
    /// Write/append a field in an existing item.
    EditItemField {
        account: Option<Account>,
        vault: Vault,
        item: Item,
        section: Option<String>,
        field: FieldTarget,
    },
}

/// 1Password account metadata displayed by the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpPickerAccount {
    pub id: String,
    pub email: String,
    pub url: String,
}

/// 1Password vault metadata displayed by the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpPickerVault {
    pub id: String,
    pub name: String,
}

/// 1Password item metadata displayed by the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpPickerItem {
    pub id: String,
    pub name: String,
    pub subtitle: String,
}

/// 1Password field metadata displayed by the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpPickerField {
    pub id: String,
    pub label: String,
    pub field_type: String,
    pub concealed: bool,
    pub reference: String,
}

/// Session-scoped metadata cache for picker drill-down panes.
pub type OpPickerCache =
    crate::op_cache::OpCache<OpPickerAccount, OpPickerVault, OpPickerItem, OpPickerField>;

/// A single row in the field-picker display list.
#[derive(Debug, Clone)]
pub enum FieldDisplayRow {
    /// A collapsible section header derived from the `op://` reference.
    SectionHeader { name: String, field_count: usize },
    /// A selectable field row. The index points into the filtered fields.
    Field { field_idx: usize },
    /// `+ New field` creation row.
    NewFieldSentinel,
    /// `+ New section` creation row.
    NewSectionSentinel,
}

#[derive(Debug, Clone, Copy)]
pub struct OpPickerAccountRef<'a> {
    pub email: &'a str,
    pub url: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct OpPickerVaultRef<'a> {
    pub id: &'a str,
    pub name: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct OpPickerItemRef<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub subtitle: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct OpPickerFieldRef<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub reference: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct OpPickerFieldDisplayRef<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub field_type: &'a str,
    pub concealed: bool,
}

pub trait OpPickerRenderState {
    fn stage(&self) -> OpPickerStage;
    fn load_state(&self) -> &OpLoadState;
    fn filter_buffer(&self) -> &str;
    fn account_count(&self) -> usize;
    fn selected_account_email(&self) -> &str;
    fn selected_vault_name(&self) -> &str;
    fn selected_item_name(&self) -> &str;
    fn selected_item_subtitle(&self) -> &str;
    fn naming_stage_input(&self) -> Option<&TextInputState<'static>>;
    fn account_lines(&self) -> Vec<Line<'static>>;
    fn vault_lines(&self) -> Vec<Line<'static>>;
    fn item_lines(&self) -> Vec<Line<'static>>;
    fn section_lines(&self) -> Vec<Line<'static>>;
    fn field_lines(&self) -> Vec<Line<'static>>;
    fn selected_index(&self) -> Option<usize>;
}

pub fn render_picker(frame: &mut Frame, area: Rect, state: &impl OpPickerRenderState) {
    frame.render_widget(ratatui::widgets::Clear, area);
    match state.load_state() {
        OpLoadState::Error(OpPickerError::Fatal(fatal)) => render_fatal(frame, area, fatal),
        OpLoadState::Loading { spinner_tick } => render_loading(frame, area, state, *spinner_tick),
        OpLoadState::Idle
        | OpLoadState::Ready
        | OpLoadState::Error(OpPickerError::Recoverable { .. }) => {
            render_pane(frame, area, state);
        }
    }
}

fn render_pane(frame: &mut Frame, area: Rect, state: &impl OpPickerRenderState) {
    let multi_account = state.account_count() > 1;

    if let Some(input) = state.naming_stage_input() {
        jackin_tui::components::text_input::render_text_input(frame, area, input);
        return;
    }

    let title = breadcrumb_title(
        state.stage(),
        multi_account,
        state.selected_account_email(),
        state.selected_vault_name(),
        state.selected_item_name(),
    );
    let title_with_spaces = format!(" {title} ");
    let block = Panel::new()
        .title(&title_with_spaces)
        .focus(PanelFocus::Focused)
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let banner_height: u16 = match state.load_state() {
        OpLoadState::Error(OpPickerError::Recoverable { .. }) => 2,
        _ => 0,
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(banner_height),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);

    if banner_height > 0
        && let OpLoadState::Error(OpPickerError::Recoverable { message }) = state.load_state()
    {
        let truncated: String = message.chars().take(120).collect();
        let line = Line::from(vec![
            Span::styled(
                "Error: ",
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            ),
            Span::styled(truncated, Style::default().fg(PHOSPHOR_DIM)),
        ]);
        frame.render_widget(Paragraph::new(line), rows[0]);
    }

    jackin_tui::components::render_filter_input(frame, rows[1], state.filter_buffer());

    let list_lines = match state.stage() {
        OpPickerStage::Account => state.account_lines(),
        OpPickerStage::Vault => state.vault_lines(),
        OpPickerStage::Item => state.item_lines(),
        OpPickerStage::Section => state.section_lines(),
        OpPickerStage::Field => state.field_lines(),
        OpPickerStage::NewItemName | OpPickerStage::FieldLabel | OpPickerStage::NewSectionName => {
            Vec::new()
        }
    };
    if list_lines.is_empty() {
        let para = Paragraph::new(Line::from(Span::styled(
            "(no matches)",
            Style::default().fg(PHOSPHOR_DIM),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(para, rows[3]);
    } else {
        render_selected_lines_in_area(frame, rows[3], list_lines, state.selected_index());
    }
}

fn render_loading(
    frame: &mut Frame,
    area: Rect,
    state: &impl OpPickerRenderState,
    tick: u8,
) {
    let multi_account = state.account_count() > 1;
    let title = breadcrumb_title(
        loading_title_stage(state.stage()),
        multi_account,
        state.selected_account_email(),
        state.selected_vault_name(),
        state.selected_item_name(),
    );
    let title_with_spaces = format!(" {title} ");
    let block = Panel::new()
        .title(&title_with_spaces)
        .focus(PanelFocus::Focused)
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let glyph = SPINNER_FRAMES[(tick as usize) % SPINNER_FRAMES.len()];
    let descriptor = loading_descriptor(
        state.stage(),
        multi_account,
        state.selected_account_email(),
        state.selected_vault_name(),
        state.selected_item_name(),
        state.selected_item_subtitle(),
    );

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    let body = Line::from(vec![
        Span::styled(glyph.to_string(), Style::default().fg(PHOSPHOR_GREEN)),
        Span::raw("  "),
        Span::styled(descriptor, Style::default().fg(PHOSPHOR_DIM)),
    ]);
    frame.render_widget(Paragraph::new(body).alignment(Alignment::Center), rows[1]);
}

pub fn render_fatal(frame: &mut Frame, area: Rect, fatal: &OpPickerFatalState) {
    let block = Panel::new()
        .title(" 1Password ")
        .focus(PanelFocus::Focused)
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    frame.render_widget(
        Paragraph::new(fatal_body_lines(fatal)).alignment(Alignment::Center),
        rows[1],
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltOpPickerRef {
    pub op: String,
    pub path: String,
    pub empty_reference_with_sibling_refs: bool,
}

/// Multi-account titles lead with the chosen account's email so the
/// operator can see which account they're drilling into; single-account
/// titles omit it.
pub fn breadcrumb_title(
    stage: OpPickerStage,
    multi_account: bool,
    account_email: &str,
    vault_name: &str,
    item_name: &str,
) -> String {
    match stage {
        OpPickerStage::Account => "1Password".to_string(),
        OpPickerStage::Vault => {
            if multi_account {
                account_email.to_string()
            } else {
                "1Password".to_string()
            }
        }
        OpPickerStage::Item
        | OpPickerStage::NewItemName
        | OpPickerStage::FieldLabel
        | OpPickerStage::NewSectionName => {
            if multi_account {
                format!("{account_email} \u{2192} {vault_name}")
            } else {
                vault_name.to_string()
            }
        }
        OpPickerStage::Section | OpPickerStage::Field => {
            if multi_account {
                format!("{account_email} \u{2192} {vault_name} \u{2192} {item_name}")
            } else {
                format!("{vault_name} \u{2192} {item_name}")
            }
        }
    }
}

/// Classifies by stderr substring because the root picker receives
/// process errors through `anyhow::Error` rather than typed variants.
pub fn classify_probe_error_message(message: impl Into<String>) -> OpPickerError {
    let message = message.into();
    if message.contains("failed to spawn") {
        OpPickerError::Fatal(OpPickerFatalState::NotInstalled)
    } else if message.contains("not signed in")
        || message.contains("not currently signed")
        || message.contains("no accounts")
    {
        OpPickerError::Fatal(OpPickerFatalState::NotSignedIn)
    } else {
        OpPickerError::Fatal(OpPickerFatalState::GenericFatal { message })
    }
}

/// Distinct sections present in loaded `op://` field references, in
/// first-appearance order, with a leading `None` (`(root)`) entry.
pub fn section_choices_from_references<'a>(
    references: impl IntoIterator<Item = &'a str>,
) -> Vec<Option<String>> {
    let mut out: Vec<Option<String>> = vec![None];
    for reference in references {
        if let Some(name) =
            crate::op_reference::parse_op_reference(reference).and_then(|parts| parts.section)
            && !out
                .iter()
                .any(|section| section.as_deref() == Some(name.as_str()))
        {
            out.push(Some(name));
        }
    }
    out
}

/// Build browse-mode field rows from the currently visible field
/// references. Returned `field_idx` values index into the visible-field
/// list supplied by the caller.
pub fn browse_field_display_rows<'a>(
    references: impl IntoIterator<Item = &'a str>,
    collapsed_sections: &HashSet<String>,
) -> Vec<FieldDisplayRow> {
    let mut unsectioned: Vec<usize> = Vec::new();
    let mut sections: Vec<(String, Vec<usize>)> = Vec::new();

    for (idx, reference) in references.into_iter().enumerate() {
        match crate::op_reference::parse_op_reference(reference).and_then(|parts| parts.section) {
            None => unsectioned.push(idx),
            Some(name) => {
                if let Some(entry) = sections.iter_mut().find(|(section, _)| section == &name) {
                    entry.1.push(idx);
                } else {
                    sections.push((name, vec![idx]));
                }
            }
        }
    }

    let mut rows = Vec::new();

    for idx in unsectioned {
        rows.push(FieldDisplayRow::Field { field_idx: idx });
    }

    for (section_name, indices) in sections {
        let count = indices.len();
        rows.push(FieldDisplayRow::SectionHeader {
            name: section_name.clone(),
            field_count: count,
        });
        if !collapsed_sections.contains(section_name.as_str()) {
            for idx in indices {
                rows.push(FieldDisplayRow::Field { field_idx: idx });
            }
        }
    }

    rows
}

/// Build create-mode field rows scoped to `selected_section`. Returned
/// `field_idx` values index into the visible-field list supplied by the
/// caller. A trailing `+ New field` sentinel is always present.
pub fn create_field_display_rows<'a>(
    references: impl IntoIterator<Item = &'a str>,
    selected_section: Option<&str>,
) -> Vec<FieldDisplayRow> {
    let mut rows: Vec<FieldDisplayRow> = references
        .into_iter()
        .enumerate()
        .filter(|(_, reference)| {
            let section =
                crate::op_reference::parse_op_reference(reference).and_then(|parts| parts.section);
            section.as_deref() == selected_section
        })
        .map(|(idx, _)| FieldDisplayRow::Field { field_idx: idx })
        .collect();
    rows.push(FieldDisplayRow::NewFieldSentinel);
    rows
}

/// Build the committed `op://` value and display path from the picker
/// cache values. UUID-form `op` segments are paired with human-readable
/// path segments, preserving a section segment from the field reference
/// when 1Password supplies one.
pub fn build_op_picker_ref<'a>(
    vault: OpPickerVaultRef<'a>,
    selected_item: OpPickerItemRef<'a>,
    items_in_vault: impl IntoIterator<Item = OpPickerItemRef<'a>>,
    field: OpPickerFieldRef<'a>,
    fields_in_item: impl IntoIterator<Item = OpPickerFieldRef<'a>>,
) -> BuiltOpPickerRef {
    let item_name_collides = items_in_vault
        .into_iter()
        .any(|item| item.id != selected_item.id && item.name == selected_item.name);
    let safe_to_embed = !selected_item.name.contains('[') && !selected_item.name.contains(']');
    let item_segment = if item_name_collides && safe_to_embed && !selected_item.subtitle.is_empty()
    {
        format!("{}[{}]", selected_item.name, selected_item.subtitle)
    } else {
        selected_item.name.to_string()
    };

    if let Some(section_name) =
        crate::op_reference::parse_op_reference(field.reference).and_then(|parts| parts.section)
    {
        return BuiltOpPickerRef {
            op: format!(
                "op://{}/{}/{}/{}",
                vault.id, selected_item.id, section_name, field.id
            ),
            path: format!(
                "{}/{}/{}/{}",
                vault.name, item_segment, section_name, field.label
            ),
            empty_reference_with_sibling_refs: false,
        };
    }

    let label = if field.label.is_empty() {
        field.id
    } else {
        field.label
    };
    let empty_reference_with_sibling_refs = field.reference.is_empty()
        && fields_in_item
            .into_iter()
            .any(|sibling| sibling.id != field.id && !sibling.reference.is_empty());

    BuiltOpPickerRef {
        op: format!("op://{}/{}/{}", vault.id, selected_item.id, field.id),
        path: format!("{}/{}/{}", vault.name, item_segment, label),
        empty_reference_with_sibling_refs,
    }
}

/// `+ New X` creation row, styled like picker list rows.
pub fn sentinel_line(text: &str, is_selected: bool) -> Line<'static> {
    let prefix = if is_selected { "\u{25b8} " } else { "  " };
    let style = if is_selected {
        Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(PHOSPHOR_DIM)
    };
    Line::from(Span::styled(format!("{prefix}{text}"), style))
}

pub fn account_lines<'a>(
    accounts: impl IntoIterator<Item = OpPickerAccountRef<'a>>,
    selected: Option<usize>,
) -> Vec<Line<'static>> {
    accounts
        .into_iter()
        .enumerate()
        .map(|(i, account)| {
            let is_selected = Some(i) == selected;
            let prefix = if is_selected { "\u{25b8} " } else { "  " };
            let label_style = if is_selected {
                Style::default()
                    .fg(PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(WHITE)
            };
            Line::from(vec![
                Span::styled(format!("{prefix}{}", account.email), label_style),
                Span::raw("  "),
                Span::styled(
                    format!("({})", account.url),
                    Style::default().fg(PHOSPHOR_DIM),
                ),
            ])
        })
        .collect()
}

pub fn vault_lines<'a>(
    vaults: impl IntoIterator<Item = OpPickerVaultRef<'a>>,
    selected: Option<usize>,
) -> Vec<Line<'static>> {
    vaults
        .into_iter()
        .enumerate()
        .map(|(i, vault)| {
            let is_selected = Some(i) == selected;
            let prefix = if is_selected { "\u{25b8} " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(WHITE)
            };
            Line::from(Span::styled(format!("{prefix}{}", vault.name), style))
        })
        .collect()
}

pub fn item_choice_lines<'a>(
    item_choices: impl IntoIterator<Item = Option<OpPickerItemRef<'a>>>,
    selected: Option<usize>,
) -> Vec<Line<'static>> {
    item_choices
        .into_iter()
        .enumerate()
        .map(|(i, choice)| {
            let is_selected = Some(i) == selected;
            choice.map_or_else(
                || sentinel_line("+ New item", is_selected),
                |item| {
                    let prefix = if is_selected { "\u{25b8} " } else { "  " };
                    let title_style = if is_selected {
                        Style::default()
                            .fg(PHOSPHOR_GREEN)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(WHITE)
                    };
                    let mut spans = vec![
                        Span::styled(prefix, title_style),
                        Span::styled(item.name.to_string(), title_style),
                    ];
                    if !item.subtitle.is_empty() {
                        let dim = Style::default().fg(PHOSPHOR_DIM);
                        spans.push(Span::styled(" (", dim));
                        spans.push(Span::styled(item.subtitle.to_string(), dim));
                        spans.push(Span::styled(")", dim));
                    }
                    Line::from(spans)
                },
            )
        })
        .collect()
}

/// Render section-stage rows: `(root)`, named sections, then a creation
/// sentinel.
pub fn section_lines(
    choices: impl IntoIterator<Item = Option<String>>,
    selected: Option<usize>,
) -> Vec<Line<'static>> {
    let choices: Vec<Option<String>> = choices.into_iter().collect();
    let sentinel_idx = choices.len();
    let mut lines: Vec<Line<'static>> = choices
        .into_iter()
        .enumerate()
        .map(|(i, choice)| {
            let is_selected = Some(i) == selected;
            let prefix = if is_selected { "\u{25b8} " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(WHITE)
            };
            let label = choice.unwrap_or_else(|| "(root)".to_string());
            Line::from(Span::styled(format!("{prefix}{label}"), style))
        })
        .collect();
    lines.push(sentinel_line(
        "+ New section",
        Some(sentinel_idx) == selected,
    ));
    lines
}

pub fn field_lines<'a>(
    rows: impl IntoIterator<Item = FieldDisplayRow>,
    fields: impl IntoIterator<Item = OpPickerFieldDisplayRef<'a>>,
    collapsed_sections: &HashSet<String>,
    selected: Option<usize>,
) -> Vec<Line<'static>> {
    let fields: Vec<OpPickerFieldDisplayRef<'a>> = fields.into_iter().collect();
    let label_w = fields
        .iter()
        .map(|field| field_display_label(*field).chars().count())
        .max()
        .unwrap_or(0)
        .max(8);

    rows.into_iter()
        .enumerate()
        .map(|(row_i, row)| {
            let is_selected = Some(row_i) == selected;
            match row {
                FieldDisplayRow::SectionHeader { name, field_count } => {
                    section_header_line(&name, field_count, collapsed_sections, is_selected)
                }
                FieldDisplayRow::Field { field_idx } => {
                    let Some(field) = fields.get(field_idx).copied() else {
                        return Line::default();
                    };
                    field_line(field, label_w, is_selected)
                }
                FieldDisplayRow::NewFieldSentinel => sentinel_line("+ New field", is_selected),
                FieldDisplayRow::NewSectionSentinel => sentinel_line("+ New section", is_selected),
            }
        })
        .collect()
}

fn section_header_line(
    name: &str,
    field_count: usize,
    collapsed_sections: &HashSet<String>,
    is_selected: bool,
) -> Line<'static> {
    let prefix = if is_selected { "\u{25b8}  " } else { "   " };
    let arrow = if collapsed_sections.contains(name) {
        "\u{25b6}"
    } else {
        "\u{25bc}"
    };
    let style = if is_selected {
        Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(PHOSPHOR_DIM)
    };
    let count_label = format!(
        "({} {})",
        field_count,
        if field_count == 1 { "field" } else { "fields" }
    );
    Line::from(vec![
        Span::styled(prefix, style),
        Span::styled(arrow, style),
        Span::styled(format!(" {name}  "), style),
        Span::styled(count_label, Style::default().fg(PHOSPHOR_DIM)),
    ])
}

fn field_line(
    field: OpPickerFieldDisplayRef<'_>,
    label_w: usize,
    is_selected: bool,
) -> Line<'static> {
    let prefix = if is_selected { "\u{25b8} " } else { "  " };
    let label = field_display_label(field);
    let pad = label_w.saturating_sub(label.chars().count());
    let label_style = if is_selected {
        Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(WHITE)
    };
    let annotation = if field.concealed {
        "(concealed)".to_string()
    } else {
        format!("({})", field.field_type.to_lowercase())
    };
    Line::from(vec![
        Span::styled(format!("{prefix}{label}"), label_style),
        Span::raw(format!("{}  ", " ".repeat(pad))),
        Span::styled(annotation, Style::default().fg(PHOSPHOR_DIM)),
    ])
}

fn field_display_label(field: OpPickerFieldDisplayRef<'_>) -> String {
    if field.label.is_empty() {
        field.id.to_string()
    } else {
        field.label.to_string()
    }
}

pub fn loading_title_stage(stage: OpPickerStage) -> OpPickerStage {
    if matches!(stage, OpPickerStage::Field) {
        OpPickerStage::Item
    } else {
        stage
    }
}

pub fn loading_descriptor(
    stage: OpPickerStage,
    multi_account: bool,
    account_email: &str,
    vault_name: &str,
    item_name: &str,
    item_subtitle: &str,
) -> String {
    match stage {
        OpPickerStage::Account => "loading accounts\u{2026}".to_string(),
        OpPickerStage::Vault => {
            if multi_account && !account_email.is_empty() {
                format!("loading vaults from {account_email}\u{2026}")
            } else {
                "loading vaults\u{2026}".to_string()
            }
        }
        OpPickerStage::Item => {
            format!("loading items from {vault_name}\u{2026}")
        }
        OpPickerStage::Field => {
            if item_subtitle.is_empty() {
                format!("loading {item_name}\u{2026}")
            } else {
                format!("loading {item_name} ({item_subtitle})\u{2026}")
            }
        }
        OpPickerStage::Section
        | OpPickerStage::NewItemName
        | OpPickerStage::FieldLabel
        | OpPickerStage::NewSectionName => "loading\u{2026}".to_string(),
    }
}

pub fn fatal_body_lines(fatal: &OpPickerFatalState) -> Vec<Line<'static>> {
    match fatal {
        OpPickerFatalState::NotInstalled => vec![
            Line::from(Span::styled(
                "1Password CLI not found.",
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Install: brew install 1password-cli (macOS)",
                Style::default().fg(PHOSPHOR_GREEN),
            )),
            Line::from(Span::styled(
                "or visit 1password.com/downloads/command-line/",
                Style::default().fg(PHOSPHOR_GREEN),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "After install, run `op signin`, then press P to retry.",
                Style::default().fg(PHOSPHOR_DIM),
            )),
        ],
        OpPickerFatalState::NotSignedIn => vec![
            Line::from(Span::styled(
                "1Password CLI is not signed in.",
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Run `op signin` in your shell, then retry.",
                Style::default().fg(PHOSPHOR_GREEN),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "jackin' uses your existing op session — there is no separate jackin' auth.",
                Style::default().fg(PHOSPHOR_DIM),
            )),
        ],
        OpPickerFatalState::NoVaults => vec![
            Line::from(Span::styled(
                "No vaults available.",
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Check 1Password's app integration settings:",
                Style::default().fg(PHOSPHOR_GREEN),
            )),
            Line::from(Span::styled(
                "Settings \u{2192} Developer \u{2192} CLI integration.",
                Style::default().fg(PHOSPHOR_GREEN),
            )),
        ],
        OpPickerFatalState::GenericFatal { message } => {
            let truncated: String = message.chars().take(120).collect();
            vec![
                Line::from(Span::styled(
                    "1Password CLI error.",
                    Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(truncated, Style::default().fg(PHOSPHOR_DIM))),
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breadcrumb_omits_pane_type_suffix_multi_account() {
        let title = breadcrumb_title(
            OpPickerStage::Vault,
            true,
            "alice@example.com",
            "ignored",
            "ignored",
        );
        assert_eq!(title, "alice@example.com");
        assert!(!title.contains("Vaults"), "no `Vaults` suffix: {title}");

        let title = breadcrumb_title(
            OpPickerStage::Item,
            true,
            "alice@example.com",
            "Personal",
            "",
        );
        assert_eq!(title, "alice@example.com \u{2192} Personal");
        assert!(!title.contains("Items"));

        let title = breadcrumb_title(
            OpPickerStage::Field,
            true,
            "alice@example.com",
            "Personal",
            "API Keys",
        );
        assert_eq!(
            title,
            "alice@example.com \u{2192} Personal \u{2192} API Keys"
        );
        assert!(!title.contains("Fields"));
    }

    #[test]
    fn breadcrumb_single_account_uses_brand_or_bare_context() {
        let v = breadcrumb_title(OpPickerStage::Vault, false, "", "Personal", "");
        assert_eq!(v, "1Password");

        let i = breadcrumb_title(OpPickerStage::Item, false, "", "Personal", "API Keys");
        assert_eq!(i, "Personal");

        let f = breadcrumb_title(OpPickerStage::Field, false, "", "Personal", "API Keys");
        assert_eq!(f, "Personal \u{2192} API Keys");
    }

    #[test]
    fn breadcrumb_account_pane_is_bare_brand() {
        let title = breadcrumb_title(OpPickerStage::Account, true, "ignored", "", "");
        assert_eq!(title, "1Password");
    }

    #[test]
    fn probe_error_message_classifies_operator_states() {
        assert!(matches!(
            classify_probe_error_message("failed to spawn op"),
            OpPickerError::Fatal(OpPickerFatalState::NotInstalled)
        ));
        assert!(matches!(
            classify_probe_error_message("not currently signed in"),
            OpPickerError::Fatal(OpPickerFatalState::NotSignedIn)
        ));
        assert!(matches!(
            classify_probe_error_message("boom"),
            OpPickerError::Fatal(OpPickerFatalState::GenericFatal { .. })
        ));
    }

    #[test]
    fn section_choices_deduplicate_in_first_seen_order() {
        let choices = section_choices_from_references([
            "op://Vault/Item/token",
            "op://Vault/Item/Auth/password",
            "op://Vault/Item/Deploy/key",
            "op://Vault/Item/Auth/otp",
        ]);
        assert_eq!(
            choices,
            vec![None, Some("Auth".to_string()), Some("Deploy".to_string())]
        );
    }

    #[test]
    fn browse_field_rows_group_sections_and_respect_collapse() {
        let mut collapsed = HashSet::new();
        collapsed.insert("Auth".to_string());
        let rows = browse_field_display_rows(
            [
                "op://Vault/Item/root",
                "op://Vault/Item/Auth/password",
                "op://Vault/Item/Auth/otp",
                "op://Vault/Item/Deploy/key",
            ],
            &collapsed,
        );
        assert!(matches!(rows[0], FieldDisplayRow::Field { field_idx: 0 }));
        assert!(matches!(
            rows[1],
            FieldDisplayRow::SectionHeader {
                ref name,
                field_count: 2
            } if name == "Auth"
        ));
        assert!(matches!(
            rows[2],
            FieldDisplayRow::SectionHeader {
                ref name,
                field_count: 1
            } if name == "Deploy"
        ));
        assert!(matches!(rows[3], FieldDisplayRow::Field { field_idx: 3 }));
    }

    #[test]
    fn create_field_rows_scope_to_section_and_add_sentinel() {
        let rows = create_field_display_rows(
            [
                "op://Vault/Item/root",
                "op://Vault/Item/Auth/password",
                "op://Vault/Item/Auth/otp",
            ],
            Some("Auth"),
        );
        assert!(matches!(rows[0], FieldDisplayRow::Field { field_idx: 1 }));
        assert!(matches!(rows[1], FieldDisplayRow::Field { field_idx: 2 }));
        assert!(matches!(rows[2], FieldDisplayRow::NewFieldSentinel));
    }

    #[test]
    fn matches_filter_accepts_empty_or_any_matching_value() {
        assert!(matches_filter("", ["anything"]));
        assert!(matches_filter("api", ["Stripe", "API token"]));
        assert!(matches_filter(
            "example",
            ["alice@example.com", "https://example.test"]
        ));
        assert!(!matches_filter("missing", ["one", "two"]));
    }

    #[test]
    fn build_op_picker_ref_uses_uuid_op_and_clean_path_for_unique_item() {
        let built = build_op_picker_ref(
            OpPickerVaultRef {
                id: "v_uuid",
                name: "Private",
            },
            OpPickerItemRef {
                id: "i_uuid",
                name: "Stripe",
                subtitle: "",
            },
            [OpPickerItemRef {
                id: "i_uuid",
                name: "Stripe",
                subtitle: "",
            }],
            OpPickerFieldRef {
                id: "f_uuid",
                label: "api key",
                reference: "op://Private/Stripe/api key",
            },
            [OpPickerFieldRef {
                id: "f_uuid",
                label: "api key",
                reference: "op://Private/Stripe/api key",
            }],
        );
        assert_eq!(built.op, "op://v_uuid/i_uuid/f_uuid");
        assert_eq!(built.path, "Private/Stripe/api key");
        assert!(!built.empty_reference_with_sibling_refs);
    }

    #[test]
    fn build_op_picker_ref_preserves_sections_and_ambiguous_subtitles() {
        let built = build_op_picker_ref(
            OpPickerVaultRef {
                id: "v_uuid",
                name: "Private",
            },
            OpPickerItemRef {
                id: "i_a",
                name: "Claude",
                subtitle: "alice@example.com",
            },
            [
                OpPickerItemRef {
                    id: "i_a",
                    name: "Claude",
                    subtitle: "alice@example.com",
                },
                OpPickerItemRef {
                    id: "i_b",
                    name: "Claude",
                    subtitle: "bob@example.com",
                },
            ],
            OpPickerFieldRef {
                id: "f_uuid",
                label: "token",
                reference: "op://Private/Claude/Auth/token",
            },
            [OpPickerFieldRef {
                id: "f_uuid",
                label: "token",
                reference: "op://Private/Claude/Auth/token",
            }],
        );
        assert_eq!(built.op, "op://v_uuid/i_a/Auth/f_uuid");
        assert_eq!(built.path, "Private/Claude[alice@example.com]/Auth/token");
    }

    #[test]
    fn build_op_picker_ref_flags_empty_reference_with_sibling_refs() {
        let built = build_op_picker_ref(
            OpPickerVaultRef {
                id: "v_uuid",
                name: "Private",
            },
            OpPickerItemRef {
                id: "i_uuid",
                name: "MyItem",
                subtitle: "",
            },
            [OpPickerItemRef {
                id: "i_uuid",
                name: "MyItem",
                subtitle: "",
            }],
            OpPickerFieldRef {
                id: "f_noref",
                label: "notes",
                reference: "",
            },
            [
                OpPickerFieldRef {
                    id: "f_noref",
                    label: "notes",
                    reference: "",
                },
                OpPickerFieldRef {
                    id: "f_sectioned",
                    label: "password",
                    reference: "op://Private/MyItem/Auth/password",
                },
            ],
        );
        assert_eq!(built.op, "op://v_uuid/i_uuid/f_noref");
        assert_eq!(built.path, "Private/MyItem/notes");
        assert!(built.empty_reference_with_sibling_refs);
    }

    #[test]
    fn section_lines_append_new_section_sentinel() {
        let lines = section_lines([None, Some("Auth".to_string())], Some(2));
        assert_eq!(lines.len(), 3);
        assert_eq!(
            lines[0].spans[0].content.as_ref(),
            "  (root)",
            "root choice renders first"
        );
        assert_eq!(
            lines[1].spans[0].content.as_ref(),
            "  Auth",
            "named section renders second"
        );
        assert_eq!(
            lines[2].spans[0].content.as_ref(),
            "\u{25b8} + New section",
            "sentinel renders last and selected"
        );
    }

    #[test]
    fn account_vault_and_item_lines_apply_selected_prefixes() {
        let account = account_lines(
            [OpPickerAccountRef {
                email: "alice@example.com",
                url: "alice.1password.com",
            }],
            Some(0),
        );
        assert_eq!(
            account[0].spans[0].content.as_ref(),
            "\u{25b8} alice@example.com"
        );
        assert_eq!(
            account[0].spans[2].content.as_ref(),
            "(alice.1password.com)"
        );

        let vault = vault_lines(
            [OpPickerVaultRef {
                id: "v1",
                name: "Private",
            }],
            None,
        );
        assert_eq!(vault[0].spans[0].content.as_ref(), "  Private");

        let items = item_choice_lines(
            [
                Some(OpPickerItemRef {
                    id: "i1",
                    name: "Claude",
                    subtitle: "alice@example.com",
                }),
                None,
            ],
            Some(1),
        );
        assert_eq!(items[0].spans[1].content.as_ref(), "Claude");
        assert_eq!(items[0].spans[3].content.as_ref(), "alice@example.com");
        assert_eq!(items[1].spans[0].content.as_ref(), "\u{25b8} + New item");
    }

    #[test]
    fn field_lines_render_headers_fields_and_sentinels() {
        let mut collapsed = HashSet::new();
        collapsed.insert("Auth".to_string());
        let lines = field_lines(
            [
                FieldDisplayRow::SectionHeader {
                    name: "Auth".to_string(),
                    field_count: 1,
                },
                FieldDisplayRow::Field { field_idx: 0 },
                FieldDisplayRow::NewFieldSentinel,
            ],
            [OpPickerFieldDisplayRef {
                id: "f1",
                label: "token",
                field_type: "CONCEALED",
                concealed: true,
            }],
            &collapsed,
            Some(1),
        );

        assert_eq!(lines[0].spans[1].content.as_ref(), "\u{25b6}");
        assert_eq!(lines[1].spans[0].content.as_ref(), "\u{25b8} token");
        assert_eq!(lines[1].spans[2].content.as_ref(), "(concealed)");
        assert_eq!(lines[2].spans[0].content.as_ref(), "  + New field");
    }

    #[test]
    fn loading_descriptor_names_current_load_target() {
        assert_eq!(
            loading_descriptor(OpPickerStage::Account, false, "", "", "", ""),
            "loading accounts\u{2026}"
        );
        assert_eq!(
            loading_descriptor(OpPickerStage::Vault, true, "alice@example.com", "", "", ""),
            "loading vaults from alice@example.com\u{2026}"
        );
        assert_eq!(
            loading_descriptor(
                OpPickerStage::Field,
                false,
                "",
                "",
                "Claude",
                "alice@example.com"
            ),
            "loading Claude (alice@example.com)\u{2026}"
        );
        assert_eq!(
            loading_title_stage(OpPickerStage::Field),
            OpPickerStage::Item
        );
    }

    #[test]
    fn fatal_body_lines_truncate_generic_errors() {
        let long = "x".repeat(140);
        let lines = fatal_body_lines(&OpPickerFatalState::GenericFatal { message: long });
        assert_eq!(lines[0].spans[0].content.as_ref(), "1Password CLI error.");
        assert_eq!(lines[2].spans[0].content.chars().count(), 120);

        let missing = fatal_body_lines(&OpPickerFatalState::NotInstalled);
        assert!(missing.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.content.contains("brew install"))
        }));
    }

    #[test]
    fn field_label_origin_maps_to_cancel_stage() {
        assert_eq!(
            FieldLabelOrigin::NewItem.cancel_stage(),
            OpPickerStage::NewItemName
        );
        assert_eq!(
            FieldLabelOrigin::NewField.cancel_stage(),
            OpPickerStage::Field
        );
        assert_eq!(
            FieldLabelOrigin::NewSection.cancel_stage(),
            OpPickerStage::NewSectionName
        );
    }

    #[test]
    fn stage_classification_separates_naming_and_filterable_lists() {
        assert!(OpPickerStage::FieldLabel.is_naming());
        assert!(OpPickerStage::NewSectionName.is_naming());
        assert!(!OpPickerStage::Field.is_naming());

        assert!(OpPickerStage::Account.is_filterable());
        assert!(OpPickerStage::Field.is_filterable());
        assert!(!OpPickerStage::Section.is_filterable());
        assert!(!OpPickerStage::FieldLabel.is_filterable());
    }
}
