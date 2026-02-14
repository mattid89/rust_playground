// ============================================================================
// Módulo de Credenciales AWS
// ============================================================================
//
// Lee y escribe el archivo ~/.aws/credentials (formato INI).
//
// CONCEPTO RUST - Ownership:
// Este módulo es un excelente ejemplo de ownership. Cuando leemos el archivo,
// el String resultante es "owned" (propiedad) de la variable. Cuando lo
// parseamos, creamos nuevos Strings para cada campo — no podemos simplemente
// guardar referencias (&str) al contenido original porque el String original
// podría ser liberado.
//
// En Node.js, los strings son garbage collected y puedes referenciarlos
// libremente. En Rust, debes ser explícito sobre quién es dueño de qué.

use crate::errors::{AppError, AppResult};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use tempfile::NamedTempFile;

/// Representa un perfil AWS con sus credenciales.
///
/// CONCEPTO RUST - Structs:
/// Similar a una interface de TypeScript, pero con datos concretos.
///   TS:   interface AwsProfile { name: string; accessKeyId: string; ... }
///   Rust: struct AwsProfile { name: String, access_key_id: String, ... }
///
/// La diferencia: en Rust, String es un buffer de bytes heap-allocated que
/// POSEE sus datos. &str sería una referencia prestada (borrowed).
#[derive(Debug, Clone)]
pub struct AwsProfile {
    /// Nombre del perfil (ej: "default", "production")
    pub name: String,
    /// AWS Access Key ID (empieza con AKIA...)
    pub access_key_id: String,
    /// AWS Secret Access Key
    pub secret_access_key: String,
    /// Región (opcional)
    pub region: Option<String>,
    /// Fecha de creación de la clave (si se puede determinar)
    pub key_created_at: Option<DateTime<Utc>>,
}

impl fmt::Display for AwsProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Mostramos solo los primeros 8 caracteres del access key por seguridad
        let masked_key = if self.access_key_id.len() > 8 {
            format!("{}...", &self.access_key_id[..8])
        } else {
            self.access_key_id.clone()
        };
        write!(f, "[{}] {}", self.name, masked_key)
    }
}

/// Contenedor del archivo completo de credenciales.
///
/// CONCEPTO RUST - HashMap:
/// Equivalente a Map<string, AwsProfile> en TypeScript.
/// La clave es el nombre del perfil, el valor es el perfil completo.
#[derive(Debug)]
pub struct CredentialsFile {
    /// Ruta al archivo de credenciales
    pub path: PathBuf,
    /// Perfiles parseados, indexados por nombre
    pub profiles: HashMap<String, AwsProfile>,
    /// Contenido original para preservar comentarios y formato
    raw_content: String,
}

impl CredentialsFile {
    /// Carga y parsea el archivo de credenciales desde la ruta por defecto.
    ///
    /// CONCEPTO RUST - Result y el operador ?:
    /// El `?` al final de una expresión es azúcar sintáctico:
    ///   - Si el Result es Ok(valor), extrae el valor y continúa
    ///   - Si el Result es Err(e), retorna inmediatamente el error
    ///
    /// En Node.js sería como:
    ///   const content = await fs.readFile(path); // throws si falla
    ///
    /// Pero en Rust es explícito — SABES dónde puede fallar porque ves el `?`
    pub fn load_default() -> AppResult<Self> {
        let path = Self::default_credentials_path()?;
        Self::load_from(&path)
    }

    /// Carga credenciales desde una ruta específica.
    ///
    /// CONCEPTO RUST - Borrowing con &PathBuf:
    /// Recibimos una REFERENCIA (&) al PathBuf. No tomamos ownership.
    /// Esto es como pasar por referencia en otros lenguajes, pero el compilador
    /// GARANTIZA que la referencia es válida durante toda la función.
    pub fn load_from(path: &PathBuf) -> AppResult<Self> {
        // `?` convierte std::io::Error -> AppError::Io automáticamente gracias a #[from]
        let raw_content = fs::read_to_string(path)?;
        let profiles = Self::parse_credentials(&raw_content)?;

        Ok(Self {
            path: path.clone(),
            profiles,
            raw_content,
        })
    }

    /// Parsea el contenido del archivo INI en perfiles.
    ///
    /// CONCEPTO RUST - String vs &str:
    /// `content: &str` recibe un "string slice" — una vista de solo lectura
    /// sobre un String. Es eficiente porque no copia los datos.
    ///
    ///   String = let s = String::from("hello")  → propiedad, heap, mutable
    ///   &str   = let s: &str = "hello"          → referencia, puede ser stack, inmutable
    ///
    /// En Node.js todos los strings son inmutables y garbage collected.
    /// En Rust, elegir entre String y &str te da control sobre allocations.
    fn parse_credentials(content: &str) -> AppResult<HashMap<String, AwsProfile>> {
        let mut profiles = HashMap::new();
        let mut current_profile: Option<String> = None;
        let mut current_key_id = String::new();
        let mut current_secret = String::new();
        let mut current_region: Option<String> = None;

        for line in content.lines() {
            let trimmed = line.trim();

            // Ignorar líneas vacías y comentarios
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
                continue;
            }

            // Detectar encabezado de sección [profile_name]
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                // Guardar perfil anterior si existía
                if let Some(ref name) = current_profile {
                    if !current_key_id.is_empty() && !current_secret.is_empty() {
                        profiles.insert(
                            name.clone(),
                            AwsProfile {
                                name: name.clone(),
                                access_key_id: current_key_id.clone(),
                                secret_access_key: current_secret.clone(),
                                region: current_region.clone(),
                                key_created_at: None,
                            },
                        );
                    }
                }

                // Iniciar nuevo perfil
                // CONCEPTO RUST - Slicing:
                // &trimmed[1..trimmed.len()-1] toma un substring.
                // Es como trimmed.slice(1, -1) en JavaScript.
                current_profile = Some(trimmed[1..trimmed.len() - 1].to_string());
                current_key_id.clear();
                current_secret.clear();
                current_region = None;
                continue;
            }

            // Parsear key = value
            if let Some((key, value)) = trimmed.split_once('=') {
                let key = key.trim();
                let value = value.trim().to_string();

                // CONCEPTO RUST - Pattern Matching:
                // `match` es como un switch, pero EXHAUSTIVO.
                // El compilador verifica que manejes todos los casos.
                // `_` es el caso "default" (catch-all).
                match key {
                    "aws_access_key_id" => current_key_id = value,
                    "aws_secret_access_key" => current_secret = value,
                    "region" => current_region = Some(value),
                    _ => {} // Ignorar claves desconocidas
                }
            }
        }

        // No olvidar el último perfil
        if let Some(ref name) = current_profile {
            if !current_key_id.is_empty() && !current_secret.is_empty() {
                profiles.insert(
                    name.clone(),
                    AwsProfile {
                        name: name.clone(),
                        access_key_id: current_key_id,
                        secret_access_key: current_secret,
                        region: current_region,
                        key_created_at: None,
                    },
                );
            }
        }

        if profiles.is_empty() {
            return Err(AppError::Parse(
                "No se encontraron perfiles válidos en el archivo de credenciales".to_string(),
            ));
        }

        Ok(profiles)
    }

    /// Actualiza un perfil con nuevas credenciales de forma ATÓMICA.
    ///
    /// CONCEPTO RUST - Escritura atómica:
    /// Escribimos a un archivo temporal y luego lo renombramos.
    /// Si el proceso se interrumpe a medio escribir, el archivo original
    /// queda intacto. Esto es critical para credenciales.
    ///
    /// En Node.js harías algo similar con write-file-atomic de npm.
    pub fn update_profile(
        &mut self,
        profile_name: &str,
        new_key_id: &str,
        new_secret: &str,
    ) -> AppResult<()> {
        // Actualizar el contenido raw reemplazando las credenciales del perfil
        let new_content =
            Self::replace_profile_credentials(&self.raw_content, profile_name, new_key_id, new_secret)?;

        // Escritura atómica: escribir a temp file, luego renombrar
        // CONCEPTO RUST - Scope y Drop:
        // El NamedTempFile se crea en el mismo directorio para garantizar
        // que el rename sea atómico (mismo filesystem).
        let dir = self
            .path
            .parent()
            .ok_or_else(|| AppError::Credentials("No se pudo determinar el directorio padre".into()))?;

        let mut temp_file = NamedTempFile::new_in(dir)?;
        temp_file.write_all(new_content.as_bytes())?;

        // `persist` renombra atómicamente el archivo temporal
        temp_file
            .persist(&self.path)
            .map_err(|e| AppError::Credentials(format!("Error al persistir archivo: {}", e)))?;

        // Actualizar estado interno
        self.raw_content = new_content;
        if let Some(profile) = self.profiles.get_mut(profile_name) {
            profile.access_key_id = new_key_id.to_string();
            profile.secret_access_key = new_secret.to_string();
            profile.key_created_at = Some(Utc::now());
        }

        Ok(())
    }

    /// Reemplaza las credenciales de un perfil en el contenido raw.
    fn replace_profile_credentials(
        content: &str,
        profile_name: &str,
        new_key_id: &str,
        new_secret: &str,
    ) -> AppResult<String> {
        let mut result = String::new();
        let mut in_target_profile = false;
        let mut found_profile = false;
        let section_header = format!("[{}]", profile_name);

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed == section_header {
                in_target_profile = true;
                found_profile = true;
                result.push_str(line);
                result.push('\n');
                continue;
            }

            // Detectar inicio de otro perfil
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                in_target_profile = false;
            }

            if in_target_profile {
                if let Some((key, _)) = trimmed.split_once('=') {
                    match key.trim() {
                        "aws_access_key_id" => {
                            result.push_str(&format!("aws_access_key_id = {}\n", new_key_id));
                            continue;
                        }
                        "aws_secret_access_key" => {
                            result.push_str(&format!("aws_secret_access_key = {}\n", new_secret));
                            continue;
                        }
                        _ => {}
                    }
                }
            }

            result.push_str(line);
            result.push('\n');
        }

        if !found_profile {
            return Err(AppError::Credentials(format!(
                "Perfil '{}' no encontrado en el archivo",
                profile_name
            )));
        }

        Ok(result)
    }

    /// Obtiene la ruta por defecto de las credenciales: ~/.aws/credentials
    pub fn default_credentials_path() -> AppResult<PathBuf> {
        // CONCEPTO RUST - Option vs null:
        // `home_dir()` retorna Option<PathBuf>:
        //   - Some(path) si se pudo determinar el home
        //   - None si no (no hay null/undefined en Rust)
        //
        // `ok_or_else` convierte Option -> Result:
        //   Some(v) -> Ok(v)
        //   None    -> Err(...)
        let home = dirs_or_home()?;
        let path = home.join(".aws").join("credentials");

        if !path.exists() {
            return Err(AppError::Credentials(format!(
                "Archivo de credenciales no encontrado en: {}",
                path.display()
            )));
        }

        Ok(path)
    }

    /// Lista todos los nombres de perfil disponibles.
    pub fn profile_names(&self) -> Vec<&str> {
        // CONCEPTO RUST - Iteradores:
        // .keys() retorna un iterador sobre las claves del HashMap.
        // .map(|k| k.as_str()) convierte cada &String a &str.
        // .collect() consume el iterador y construye un Vec.
        //
        // En Node.js: Object.keys(profiles)
        // En Rust: profiles.keys().map(...).collect()
        //
        // Los iteradores en Rust son lazy y zero-cost — el compilador
        // los optimiza como si hubieras escrito un for loop manual.
        let mut names: Vec<&str> = self.profiles.keys().map(|k| k.as_str()).collect();
        names.sort();
        names
    }

    /// Obtiene un perfil por nombre.
    pub fn get_profile(&self, name: &str) -> Option<&AwsProfile> {
        self.profiles.get(name)
    }
}

/// Helper para obtener el directorio home del usuario.
fn dirs_or_home() -> AppResult<PathBuf> {
    // Intentamos la variable de entorno HOME primero
    std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| AppError::Config("No se pudo determinar el directorio home del usuario".into()))
}

// ============================================================================
// Tests
// ============================================================================
//
// CONCEPTO RUST - Testing:
// En Rust, los tests viven junto al código que testean.
// `#[cfg(test)]` le dice al compilador: "solo compila esto en modo test".
// Esto es como tener los .test.js al lado del código, pero integrado en el lenguaje.

#[cfg(test)]
mod tests {
    use super::*;

    /// Contenido de ejemplo para tests
    fn sample_credentials() -> &'static str {
        "[default]\naws_access_key_id = AKIAIOSFODNN7EXAMPLE\naws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY\nregion = us-east-1\n\n[production]\naws_access_key_id = AKIAI44QH8DHBEXAMPLE\naws_secret_access_key = je7MtGbClwBF/2Zp9Utk/h3yCo8nvbEXAMPLEKEY\n"
    }

    #[test]
    fn test_parse_credentials_valid() {
        let profiles = CredentialsFile::parse_credentials(sample_credentials()).unwrap();
        assert_eq!(profiles.len(), 2);
        assert!(profiles.contains_key("default"));
        assert!(profiles.contains_key("production"));
    }

    #[test]
    fn test_parse_credentials_values() {
        let profiles = CredentialsFile::parse_credentials(sample_credentials()).unwrap();
        let default = profiles.get("default").unwrap();
        assert_eq!(default.access_key_id, "AKIAIOSFODNN7EXAMPLE");
        assert_eq!(default.region, Some("us-east-1".to_string()));
    }

    #[test]
    fn test_parse_empty_file() {
        let result = CredentialsFile::parse_credentials("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_comments_ignored() {
        let content = "# This is a comment\n[default]\n; Another comment\naws_access_key_id = AKIAIOSFODNN7EXAMPLE\naws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY\n";
        let profiles = CredentialsFile::parse_credentials(content).unwrap();
        assert_eq!(profiles.len(), 1);
    }

    #[test]
    fn test_replace_profile_credentials() {
        let new_content = CredentialsFile::replace_profile_credentials(
            sample_credentials(),
            "default",
            "AKIANEWKEYID12345678",
            "newSecretKeyHere12345678901234567890",
        )
        .unwrap();

        assert!(new_content.contains("AKIANEWKEYID12345678"));
        assert!(new_content.contains("newSecretKeyHere12345678901234567890"));
        // El perfil production no debe cambiar
        assert!(new_content.contains("AKIAI44QH8DHBEXAMPLE"));
    }

    #[test]
    fn test_replace_nonexistent_profile() {
        let result = CredentialsFile::replace_profile_credentials(
            sample_credentials(),
            "nonexistent",
            "AKIANEWKEYID",
            "newSecret",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_profile_display_masks_key() {
        let profile = AwsProfile {
            name: "test".to_string(),
            access_key_id: "AKIAIOSFODNN7EXAMPLE".to_string(),
            secret_access_key: "secret".to_string(),
            region: None,
            key_created_at: None,
        };
        let display = format!("{}", profile);
        assert!(display.contains("AKIAIОСF") || display.contains("AKIAIOSF"));
        assert!(!display.contains("AKIAIOSFODNN7EXAMPLE"));
    }
}
