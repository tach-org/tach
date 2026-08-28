use crate::{
    cli::{create_clickable_link, fail, warning},
    diagnostics::{CodeDiagnostic, Diagnostic, DiagnosticDetails, Severity},
};
use std::collections::HashMap;

use console::style;
use itertools::Itertools;

#[derive(Debug, PartialEq, Eq, Ord, PartialOrd, Hash, Clone)]
enum DiagnosticGroupKind {
    Other,
    Configuration,
    Deadcode,
    ExternalDependency,
    Interface,
    InternalDependency,
}

impl From<&DiagnosticDetails> for DiagnosticGroupKind {
    fn from(details: &DiagnosticDetails) -> Self {
        match details {
            DiagnosticDetails::Configuration(..) => Self::Configuration,
            DiagnosticDetails::Code(code_diagnostic_details) => match code_diagnostic_details {
                CodeDiagnostic::UndeclaredDependency { .. }
                | CodeDiagnostic::DeprecatedDependency { .. }
                | CodeDiagnostic::ForbiddenDependency { .. }
                | CodeDiagnostic::LayerViolation { .. }
                | CodeDiagnostic::ClosedLayerViolation { .. } => Self::InternalDependency,

                CodeDiagnostic::PrivateDependency { .. }
                | CodeDiagnostic::InvalidDataTypeExport { .. } => Self::Interface,

                CodeDiagnostic::UndeclaredExternalDependency { .. }
                | CodeDiagnostic::ModuleUndeclaredExternalDependency { .. }
                | CodeDiagnostic::ModuleForbiddenExternalDependency { .. }
                | CodeDiagnostic::UnusedExternalDependency { .. } => Self::ExternalDependency,

                CodeDiagnostic::DeadFile { .. } | CodeDiagnostic::DeadSymbol { .. } => {
                    Self::Deadcode
                }

                CodeDiagnostic::UnnecessarilyIgnoredDependency { .. }
                | CodeDiagnostic::UnusedIgnoreDirective()
                | CodeDiagnostic::MissingIgnoreDirectiveReason() => Self::Other,
            },
        }
    }
}

#[derive(Debug)]
struct DiagnosticGroup<'a> {
    kind: DiagnosticGroupKind,
    severity: Severity,
    header: String,
    diagnostics: Vec<&'a Diagnostic>,
    footer: Option<String>,
}

impl<'a> DiagnosticGroup<'a> {
    fn new(severity: Severity, kind: DiagnosticGroupKind) -> Self {
        let (header, footer) = match kind {
            DiagnosticGroupKind::Configuration => (style("Configuration").red().bold(), None),
            DiagnosticGroupKind::InternalDependency => (
                style("Internal Dependencies").red().bold(),
                Some(style(
                    "If you intended to add a new dependency, run 'tach sync' to update your module configuration.\n\
                    Otherwise, remove any disallowed imports and consider refactoring."
                ).yellow()),
            ),
            DiagnosticGroupKind::ExternalDependency => (
                style("External Dependencies").red().bold(),
                Some(style(
                    "Consider updating the corresponding pyproject.toml file,\n\
                    or add the dependencies to the 'external.exclude' list in tach.toml."
                ).yellow()),
            ),
            DiagnosticGroupKind::Deadcode => (
                style("Dead Code").red().bold(),
                Some(style(
                    "Adjust deadcode entry points, public API settings, or ignore patterns in tach.toml if this code is intentionally reachable."
                ).yellow()),
            ),
            DiagnosticGroupKind::Interface => (
                style("Interfaces").red().bold(),
                Some(style(
                    "If you intended to change an interface, edit the '[[interfaces]]' section of tach.toml.\n\
                    Otherwise, remove any disallowed imports and consider refactoring."
                ).yellow()),
            ),
            DiagnosticGroupKind::Other => (style("General").red().bold(), None),
        };

        Self {
            kind,
            severity,
            header: header.to_string(),
            diagnostics: vec![],
            footer: footer.map(|f| f.to_string()),
        }
    }

    fn add_diagnostic(&mut self, diagnostic: &'a Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    fn sort_diagnostics(&mut self) {
        self.diagnostics.sort_by(|a, b| {
            // First sort by severity (warnings first)
            let severity_order = b.severity().cmp(&a.severity());
            if severity_order != std::cmp::Ordering::Equal {
                return severity_order;
            }

            // Then sort by file path (None first)
            match (a.file_path(), b.file_path()) {
                (None, None) => std::cmp::Ordering::Equal,
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (Some(a_path), Some(b_path)) => a_path.cmp(b_path),
            }
        });
    }
}

fn format_diagnostic(diagnostic: &Diagnostic) -> String {
    let local_error_path = diagnostic.file_path();

    let error_location = match local_error_path {
        Some(path) => create_clickable_link(path, &diagnostic.line_number().unwrap()),
        None => diagnostic.severity().to_string(),
    };

    match diagnostic.severity() {
        Severity::Error => format!(
            "{} {}{} {}",
            fail(),
            style(error_location).red().bold(),
            style(":").yellow().bold(),
            style(diagnostic.message()).yellow(),
        ),
        Severity::Warning => format!(
            "{} {}{} {}",
            warning(),
            style(error_location).yellow().bold(),
            style(":").yellow().bold(),
            style(diagnostic.message()).yellow(),
        ),
    }
}

fn format_diagnostic_group(group: &mut DiagnosticGroup) -> String {
    group.sort_diagnostics();
    let header = match group.severity {
        Severity::Error => style(&group.header).red().bold(),
        Severity::Warning => style(&group.header).yellow().bold(),
    };
    let diagnostics = group
        .diagnostics
        .iter()
        .map(|d| format_diagnostic(d))
        .collect::<Vec<String>>()
        .join("\n");

    match &group.footer {
        Some(footer) => format!("{header}\n{diagnostics}\n\n{footer}"),
        None => format!("{header}\n{diagnostics}"),
    }
}

pub fn format_diagnostics(diagnostics: &[Diagnostic]) -> String {
    let mut groups: HashMap<DiagnosticGroupKind, DiagnosticGroup> = HashMap::new();

    for diagnostic in diagnostics {
        let group_kind = DiagnosticGroupKind::from(diagnostic.details());
        let group = groups
            .entry(group_kind.clone())
            .or_insert_with(|| DiagnosticGroup::new(diagnostic.severity(), group_kind));
        group.add_diagnostic(diagnostic);
    }

    let mut formatted_diagnostics = Vec::new();
    for group in groups
        .values_mut()
        .sorted_by_key(|group| group.kind.clone())
    {
        formatted_diagnostics.push(format_diagnostic_group(group));
    }

    formatted_diagnostics.join("\n\n")
}
