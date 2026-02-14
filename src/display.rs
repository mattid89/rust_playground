// ============================================================================
// Módulo de Display - Formateo de salida
// ============================================================================
//
// Funciones para mostrar resultados formateados en la terminal.
// Usa tabled para tablas bonitas y colored para colores.

use crate::rotator::KeyStatus;
use colored::Colorize;
use tabled::{Table, Tabled};

/// Fila de la tabla de estado de credenciales.
///
/// CONCEPTO RUST - Derive:
/// `#[derive(Tabled)]` genera automáticamente el código para convertir
/// este struct en una fila de tabla. Similar a como funcionan los decoradores
/// de class-transformer en Node.js/TypeScript.
#[derive(Tabled)]
struct StatusRow {
    #[tabled(rename = "Perfil")]
    profile: String,
    #[tabled(rename = "Access Key")]
    access_key: String,
    #[tabled(rename = "Antigüedad")]
    age: String,
    #[tabled(rename = "Máx. Días")]
    max_age: String,
    #[tabled(rename = "Estado")]
    status: String,
    #[tabled(rename = "Creada")]
    created: String,
}

/// Muestra una tabla con el estado de las credenciales.
///
/// CONCEPTO RUST - Slice &[KeyStatus]:
/// `&[KeyStatus]` es un "slice" — una vista sobre un array/vector.
/// Es como recibir un array readonly en TypeScript.
///
///   TypeScript: function display(statuses: readonly KeyStatus[])
///   Rust:       fn display_status_table(statuses: &[KeyStatus])
///
/// El slice no posee los datos, solo los referencia.
/// Es más eficiente que clonar el vector completo.
pub fn display_status_table(statuses: &[KeyStatus]) {
    let rows: Vec<StatusRow> = statuses
        .iter()
        .map(|s| {
            let age_display = if s.age_days >= 0 {
                format!("{} días", s.age_days)
            } else {
                "desconocido".to_string()
            };

            let created_display = s
                .created_date
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "N/A".to_string());

            StatusRow {
                profile: s.profile_name.clone(),
                access_key: format!("{}...", &s.access_key_id[..8.min(s.access_key_id.len())]),
                age: age_display,
                max_age: format!("{} días", s.max_age_days),
                status: s.status_display(),
                created: created_display,
            }
        })
        .collect();

    if rows.is_empty() {
        println!("{}", "No se encontraron credenciales para verificar".yellow());
        return;
    }

    let table = Table::new(rows).to_string();
    println!("\n{}\n", table);
}

/// Muestra un resumen después de verificar credenciales.
pub fn display_check_summary(statuses: &[KeyStatus]) {
    let total = statuses.len();
    let needs_rotation = statuses.iter().filter(|s| s.needs_rotation).count();
    let ok = total - needs_rotation;

    println!(
        "{}",
        format!("Resumen: {} perfiles verificados", total)
            .bold()
    );

    if ok > 0 {
        println!("  {} {} perfiles con credenciales vigentes", "✓".green(), ok);
    }
    if needs_rotation > 0 {
        println!(
            "  {} {} perfiles necesitan rotación",
            "✗".red().bold(),
            needs_rotation
        );
        println!(
            "\n  Ejecuta {} para rotarlas",
            "aws-key-rotator rotate --all".cyan()
        );
    } else {
        println!(
            "\n  {}",
            "Todas las credenciales están al día".green().bold()
        );
    }
}
