// ============================================================================
// Módulo de Programación Automática
// ============================================================================
//
// Configura rotación automática de claves vía cron (Linux/macOS).
//
// CONCEPTO RUST - std::process::Command:
// Equivalente a child_process.exec() de Node.js.
// Permite ejecutar comandos del sistema operativo.

use crate::errors::{AppError, AppResult};
use colored::Colorize;
use std::process::Command;

const CRON_COMMENT: &str = "# aws-key-rotator: rotación automática de credenciales";

/// Configura un cron job para rotar las claves automáticamente.
///
/// CONCEPTO RUST - String formatting:
/// `format!()` es como template literals en JavaScript:
///   JS:   `cron entry: ${schedule} ${command}`
///   Rust: format!("cron entry: {} {}", schedule, command)
///
/// La diferencia: format! verifica los tipos en compile time.
/// Si pasas un número donde espera un string, no compila.
pub fn setup_cron(interval_days: u32) -> AppResult<()> {
    // Obtener el path del ejecutable actual
    let exe_path = std::env::current_exe()
        .map_err(|e| AppError::Config(format!("No se pudo obtener el path del ejecutable: {}", e)))?;

    let cron_schedule = match interval_days {
        1 => "0 0 * * *".to_string(),       // Diario
        7 => "0 0 * * 0".to_string(),       // Semanal
        30 => "0 0 1 * *".to_string(),      // Mensual
        _ => format!("0 0 */{} * *", interval_days), // Cada N días
    };

    let cron_entry = format!(
        "{}\n{} {} rotate --all >> /tmp/aws-key-rotator.log 2>&1",
        CRON_COMMENT,
        cron_schedule,
        exe_path.display()
    );

    // Leer crontab actual
    let current_crontab = Command::new("crontab")
        .arg("-l")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    // Verificar si ya existe una entrada
    if current_crontab.contains("aws-key-rotator") {
        println!(
            "{}",
            "Ya existe una entrada de aws-key-rotator en crontab. Actualizando..."
                .yellow()
        );
        // Filtrar la entrada existente
        let filtered: Vec<&str> = current_crontab
            .lines()
            .filter(|line| !line.contains("aws-key-rotator"))
            .collect();
        let new_crontab = format!("{}\n{}\n", filtered.join("\n"), cron_entry);
        install_crontab(&new_crontab)?;
    } else {
        let new_crontab = format!("{}\n{}\n", current_crontab.trim(), cron_entry);
        install_crontab(&new_crontab)?;
    }

    println!(
        "{}",
        format!(
            "Rotación automática configurada cada {} días",
            interval_days
        )
        .green()
        .bold()
    );
    println!("  Logs en: /tmp/aws-key-rotator.log");

    Ok(())
}

/// Desactiva la rotación automática eliminando la entrada de cron.
pub fn disable_cron() -> AppResult<()> {
    let current_crontab = Command::new("crontab")
        .arg("-l")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    if !current_crontab.contains("aws-key-rotator") {
        println!("{}", "No hay rotación automática configurada".yellow());
        return Ok(());
    }

    let filtered: Vec<&str> = current_crontab
        .lines()
        .filter(|line| !line.contains("aws-key-rotator"))
        .collect();

    let new_crontab = format!("{}\n", filtered.join("\n"));
    install_crontab(&new_crontab)?;

    println!(
        "{}",
        "Rotación automática desactivada".green().bold()
    );

    Ok(())
}

/// Instala un nuevo crontab.
///
/// CONCEPTO RUST - Process I/O:
/// Usamos stdin pipe para enviar datos al proceso, similar a:
///   Node.js: const proc = spawn('crontab', ['-']); proc.stdin.write(data);
fn install_crontab(content: &str) -> AppResult<()> {
    use std::io::Write;

    let mut child = Command::new("crontab")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .spawn()?;

    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(content.as_bytes())?;
    }

    let status = child.wait()?;

    if !status.success() {
        return Err(AppError::Config(
            "Error al instalar crontab".to_string(),
        ));
    }

    Ok(())
}
