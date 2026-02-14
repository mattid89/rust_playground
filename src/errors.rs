// ============================================================================
// Módulo de Errores Personalizados
// ============================================================================
//
// CONCEPTO RUST - Error Handling:
// En Node.js usas try/catch con throw. En Rust, los errores son VALORES
// que se retornan explícitamente con Result<T, E>.
//
// `thiserror` genera automáticamente la implementación de `std::error::Error`,
// similar a crear clases que extienden Error en Node.js:
//
//   Node.js:   class CredentialsError extends Error { ... }
//   Rust:      #[derive(Error)] enum AppError { Credentials(...) }
//
// La diferencia clave: en Rust el compilador te OBLIGA a manejar cada error.
// No hay excepciones no capturadas que tumben tu aplicación a las 3am.

use thiserror::Error;

/// Todos los errores posibles de la aplicación.
///
/// CONCEPTO RUST - Enums:
/// Los enums en Rust son mucho más poderosos que en TypeScript.
/// Cada variante puede contener datos diferentes (tagged unions).
/// En TS sería algo como: type AppError = { kind: "credentials", msg: string } | { kind: "aws", ... }
#[derive(Debug, Error)]
pub enum AppError {
    /// Error al leer/escribir el archivo de credenciales
    #[error("Error de credenciales: {0}")]
    Credentials(String),

    /// Error al comunicarse con AWS
    #[error("Error de AWS IAM: {0}")]
    AwsIam(String),

    /// Error al parsear el archivo de credenciales
    #[error("Error de parseo: {0}")]
    Parse(String),

    /// Error de I/O del sistema de archivos
    #[error("Error de I/O: {0}")]
    Io(#[from] std::io::Error),

    /// Error genérico de configuración
    #[error("Error de configuración: {0}")]
    Config(String),
}

// CONCEPTO RUST - From trait:
// `#[from]` genera automáticamente una implementación de `From<std::io::Error>`,
// permitiendo que los errores de I/O se conviertan automáticamente en AppError
// cuando usas el operador `?`. Esto es como hacer un wrapper automático.
//
// Ejemplo:
//   let content = std::fs::read_to_string("file")?;
//   // Si falla, std::io::Error se convierte automáticamente en AppError::Io(...)

/// Alias para Result que usa nuestro tipo de error.
/// Esto evita escribir Result<T, AppError> en cada función.
///
/// En Node.js no hay equivalente directo, pero sería como tener un tipo global:
///   type AppResult<T> = { ok: T } | { error: AppError }
pub type AppResult<T> = std::result::Result<T, AppError>;
