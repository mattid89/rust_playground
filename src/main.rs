// ============================================================================
// AWS Key Rotator - Punto de entrada
// ============================================================================
//
// Herramienta CLI para gestionar y rotar automáticamente credenciales AWS.
//
// CONCEPTO RUST - Módulos:
// En Node.js importas con: const x = require('./x') o import x from './x'
// En Rust declaras módulos con `mod` y los usas con `use`.
//
// La diferencia: Rust tiene un sistema de módulos jerárquico.
// `mod credentials;` busca el archivo src/credentials.rs
// `use crate::credentials::CredentialsFile;` importa un tipo específico.
//
// `crate` es como el "root" del proyecto. Similar a usar @ en path aliases de TS.

mod cli;
mod credentials;
mod display;
mod errors;
mod rotator;
mod scheduler;

use clap::Parser;
use cli::{Cli, Commands};
use colored::Colorize;
use credentials::CredentialsFile;
use errors::AppResult;
use rotator::KeyRotator;
use tracing::info;

/// CONCEPTO RUST - #[tokio::main]:
/// Esta macro transforma `async fn main()` en un `fn main()` normal
/// que inicializa el runtime de Tokio y ejecuta el futuro.
///
/// Es como si en Node.js tuvieras que escribir:
///   const runtime = new AsyncRuntime();
///   runtime.run(async () => { ... });
///
/// Pero Tokio lo hace automáticamente con esta anotación.
#[tokio::main]
async fn main() {
    // Parsear argumentos CLI
    let cli = Cli::parse();

    // Configurar logging basado en el nivel de verbosidad
    setup_logging(cli.verbose);

    info!("AWS Key Rotator iniciado");

    // Ejecutar el comando solicitado
    // CONCEPTO RUST - if let Err(e):
    // Combinación de pattern matching con if.
    // Solo ejecuta el bloque si el resultado es un error.
    if let Err(e) = run(cli).await {
        eprintln!("{} {}", "Error:".red().bold(), e);
        std::process::exit(1);
    }
}

/// Función principal que despacha al subcomando correcto.
///
/// CONCEPTO RUST - Pattern matching exhaustivo:
/// El `match` sobre Commands OBLIGA a manejar todos los subcomandos.
/// Si agregas un nuevo subcomando al enum y olvidas manejarlo aquí,
/// el compilador te dará un error. Esto es imposible en JavaScript.
async fn run(cli: Cli) -> AppResult<()> {
    match cli.command {
        Commands::Check { max_age, profile } => {
            cmd_check(cli.credentials_file, max_age, profile).await
        }
        Commands::Rotate {
            profile,
            all,
            keep_old,
            dry_run,
        } => {
            cmd_rotate(cli.credentials_file, profile, all, keep_old, dry_run).await
        }
        Commands::Schedule { interval, disable } => {
            if disable {
                scheduler::disable_cron()
            } else {
                scheduler::setup_cron(interval)
            }
        }
    }
}

/// Comando: check - Verifica la antigüedad de las credenciales.
async fn cmd_check(
    credentials_path: Option<String>,
    max_age: u32,
    profile_filter: Option<String>,
) -> AppResult<()> {
    println!(
        "{}",
        "Verificando antigüedad de credenciales AWS...".cyan().bold()
    );

    let creds = load_credentials(credentials_path)?;
    let rotator = KeyRotator::new().await?;

    let profiles_to_check: Vec<&str> = match &profile_filter {
        Some(name) => {
            if creds.get_profile(name).is_none() {
                return Err(errors::AppError::Credentials(format!(
                    "Perfil '{}' no encontrado. Disponibles: {}",
                    name,
                    creds.profile_names().join(", ")
                )));
            }
            vec![name.as_str()]
        }
        None => creds.profile_names(),
    };

    let mut statuses = Vec::new();

    for profile_name in &profiles_to_check {
        let profile = creds.get_profile(profile_name).unwrap();

        match rotator.check_key_age(profile, max_age).await {
            Ok(status) => statuses.push(status),
            Err(e) => {
                eprintln!(
                    "  {} Error verificando perfil '{}': {}",
                    "⚠".yellow(),
                    profile_name,
                    e
                );
            }
        }
    }

    display::display_status_table(&statuses);
    display::display_check_summary(&statuses);

    Ok(())
}

/// Comando: rotate - Rota las claves de acceso.
async fn cmd_rotate(
    credentials_path: Option<String>,
    profile: Option<String>,
    all: bool,
    keep_old: bool,
    dry_run: bool,
) -> AppResult<()> {
    if dry_run {
        println!("{}", "[DRY RUN] Simulando rotación...".yellow().bold());
    }

    let mut creds = load_credentials(credentials_path)?;

    // CONCEPTO RUST - Vec<String> ownership:
    // Creamos un Vec<String> que POSEE los nombres de los perfiles.
    // No podemos guardar &str porque `creds` será mutado más adelante
    // (al actualizar credenciales), lo que invalidaría las referencias.
    let profiles_to_rotate: Vec<String> = if all {
        creds.profile_names().iter().map(|s| s.to_string()).collect()
    } else if let Some(name) = profile {
        if creds.get_profile(&name).is_none() {
            return Err(errors::AppError::Credentials(format!(
                "Perfil '{}' no encontrado",
                name
            )));
        }
        vec![name]
    } else {
        return Err(errors::AppError::Config(
            "Especifica --profile <nombre> o --all".to_string(),
        ));
    };

    println!(
        "Perfiles a rotar: {}",
        profiles_to_rotate.join(", ").cyan()
    );

    let mut success_count = 0;
    let mut error_count = 0;

    for profile_name in &profiles_to_rotate {
        println!("\n{}", "─".repeat(50));

        let rotator = KeyRotator::with_profile(profile_name).await?;

        // Verificar que no haya ya 2 keys (máximo de AWS)
        match rotator.count_existing_keys().await {
            Ok(count) if count >= 2 && !keep_old => {
                eprintln!(
                    "  {} El perfil '{}' ya tiene {} access keys (máximo AWS: 2).",
                    "⚠".yellow(),
                    profile_name,
                    count
                );
                eprintln!(
                    "    Elimina una clave manualmente o usa --keep-old con precaución."
                );
                error_count += 1;
                continue;
            }
            Err(e) => {
                eprintln!(
                    "  {} Error verificando keys existentes para '{}': {}",
                    "✗".red(),
                    profile_name,
                    e
                );
                error_count += 1;
                continue;
            }
            _ => {}
        }

        match rotator
            .rotate_key(&mut creds, profile_name, keep_old, dry_run)
            .await
        {
            Ok(()) => success_count += 1,
            Err(e) => {
                eprintln!(
                    "  {} Error rotando '{}': {}",
                    "✗".red().bold(),
                    profile_name,
                    e
                );
                error_count += 1;
            }
        }
    }

    println!("\n{}", "─".repeat(50));
    println!(
        "\n{}: {} exitosas, {} errores",
        "Rotación completada".bold(),
        success_count.to_string().green(),
        if error_count > 0 {
            error_count.to_string().red().to_string()
        } else {
            "0".to_string()
        }
    );

    Ok(())
}

/// Carga el archivo de credenciales desde la ruta especificada o la ruta por defecto.
fn load_credentials(custom_path: Option<String>) -> AppResult<CredentialsFile> {
    match custom_path {
        Some(path) => {
            let path_buf = std::path::PathBuf::from(&path);
            CredentialsFile::load_from(&path_buf)
        }
        None => CredentialsFile::load_default(),
    }
}

/// Configura el sistema de logging basado en el nivel de verbosidad.
///
/// CONCEPTO RUST - tracing:
/// `tracing` es el estándar de facto para logging en Rust async.
/// Es como winston/pino pero con soporte nativo para contexto async.
///
///   -v   = info  (operaciones principales)
///   -vv  = debug (detalles de cada paso)
///   -vvv = trace (todo, incluyendo datos sensibles parciales)
fn setup_logging(verbosity: u8) {
    let filter = match verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .with_target(false)
        .with_thread_ids(false)
        .init();
}
