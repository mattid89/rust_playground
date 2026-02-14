// ============================================================================
// Módulo CLI - Interfaz de Línea de Comandos
// ============================================================================
//
// CONCEPTO RUST - Derive macros:
// Clap usa derive macros para generar el parser automáticamente desde structs.
// Es similar a decoradores en TypeScript, pero se ejecuta en tiempo de compilación.
//
//   TypeScript + class-validator:
//     @IsString() @MinLength(3) name: string;
//
//   Rust + clap:
//     #[arg(short, long)] name: String
//
// La diferencia: en Rust la validación ocurre en COMPILE TIME cuando es posible.

use clap::{Parser, Subcommand};

/// AWS Key Rotator - Gestión segura de credenciales AWS
///
/// Herramienta CLI para verificar, rotar y gestionar automáticamente
/// las claves de acceso AWS IAM.
#[derive(Parser, Debug)]
#[command(
    name = "aws-key-rotator",
    version,
    about = "Gestión segura y rotación automática de credenciales AWS",
    long_about = "AWS Key Rotator es una herramienta CLI que ayuda a mantener seguras\ntus credenciales AWS rotando automáticamente las access keys.\n\nSoporta múltiples perfiles, verificación de antigüedad y rotación atómica."
)]
pub struct Cli {
    /// Subcomando a ejecutar
    #[command(subcommand)]
    pub command: Commands,

    /// Ruta personalizada al archivo de credenciales
    /// (por defecto: ~/.aws/credentials)
    #[arg(short = 'f', long, global = true)]
    pub credentials_file: Option<String>,

    /// Nivel de verbosidad para logs (-v, -vv, -vvv)
    ///
    /// CONCEPTO RUST - count:
    /// `action = ArgAction::Count` cuenta cuántas veces aparece el flag.
    /// -v = 1 (info), -vv = 2 (debug), -vvv = 3 (trace)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,
}

/// Comandos disponibles.
///
/// CONCEPTO RUST - Enums con datos:
/// Cada variante del enum puede contener diferentes datos.
/// Esto es imposible con enums de TypeScript (que son solo números o strings).
///
///   TypeScript: enum Command { Check = "check", Rotate = "rotate" }
///   // No puedes adjuntar datos diferentes a cada variante
///
///   Rust: enum Commands { Check { max_age: u32 }, Rotate { profile: Option<String> } }
///   // Cada variante es como su propio struct
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Verificar la antigüedad de las credenciales
    ///
    /// Muestra una tabla con el estado de todas las claves,
    /// indicando cuáles necesitan rotación.
    Check {
        /// Antigüedad máxima en días antes de considerar una clave como expirada
        #[arg(short, long, default_value = "90")]
        max_age: u32,

        /// Verificar solo un perfil específico
        #[arg(short, long)]
        profile: Option<String>,
    },

    /// Rotar las claves de acceso
    ///
    /// Crea nuevas claves, actualiza el archivo de credenciales,
    /// y elimina las claves antiguas después de validar.
    Rotate {
        /// Nombre del perfil a rotar
        #[arg(short, long, conflicts_with = "all")]
        profile: Option<String>,

        /// Rotar todos los perfiles
        #[arg(short, long)]
        all: bool,

        /// No eliminar las claves antiguas después de rotar
        #[arg(long)]
        keep_old: bool,

        /// Modo dry-run: muestra qué haría sin ejecutar cambios
        #[arg(short, long)]
        dry_run: bool,
    },

    /// Configurar rotación automática (vía cron/systemd)
    Schedule {
        /// Intervalo en días entre rotaciones
        #[arg(short, long, default_value = "30")]
        interval: u32,

        /// Desactivar la rotación programada
        #[arg(short, long)]
        disable: bool,
    },
}
