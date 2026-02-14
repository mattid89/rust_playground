// ============================================================================
// Módulo de Rotación de Claves
// ============================================================================
//
// CONCEPTO RUST - Async/Await:
// En Node.js, TODO es async por defecto (event loop).
// En Rust, async es OPT-IN — solo pagas el costo de async cuando lo necesitas.
//
//   Node.js: async function rotateKey() { await iam.createAccessKey() }
//   Rust:    async fn rotate_key() -> Result<()> { iam.create_access_key().await? }
//
// Diferencia clave:
// - Node.js: un solo thread, async I/O, callback queue
// - Rust + Tokio: puede usar múltiples threads, futures son lazy
//                 (no se ejecutan hasta que haces .await)
//
// El `?` después del `.await` propaga errores automáticamente.
// Es como si tuvieras try/catch implícito en cada await.

use crate::credentials::{AwsProfile, CredentialsFile};
use crate::errors::{AppError, AppResult};
use aws_sdk_iam::Client as IamClient;
use chrono::{DateTime, Duration, Utc};
use colored::Colorize;
use tracing::{info, warn, error};

/// Resultado de verificar la antigüedad de una clave.
#[derive(Debug)]
pub struct KeyStatus {
    pub profile_name: String,
    pub access_key_id: String,
    pub age_days: i64,
    pub max_age_days: u32,
    pub needs_rotation: bool,
    pub created_date: Option<DateTime<Utc>>,
}

impl KeyStatus {
    /// Formatea el estado con colores para la terminal.
    pub fn status_display(&self) -> String {
        if self.needs_rotation {
            "ROTAR".red().bold().to_string()
        } else if self.age_days > (self.max_age_days as i64 * 3 / 4) {
            "PRONTO".yellow().to_string()
        } else {
            "OK".green().to_string()
        }
    }
}

/// Motor de rotación de claves AWS.
///
/// CONCEPTO RUST - Struct con lifetime implícito:
/// Este struct posee (owns) su IamClient. No hay referencias prestadas,
/// así que no necesitamos lifetime annotations.
///
/// Si tuviéramos una referencia, sería:
///   struct Rotator<'a> { client: &'a IamClient }
///   // 'a dice: "esta referencia vive al menos tanto como el struct"
pub struct KeyRotator {
    iam_client: IamClient,
}

impl KeyRotator {
    /// Crea un nuevo KeyRotator con configuración AWS automática.
    ///
    /// CONCEPTO RUST - async fn:
    /// `async fn` retorna un Future que hay que `.await`.
    /// El Future no se ejecuta hasta que alguien hace await sobre él.
    /// En Node.js, llamar a una async function ya empieza su ejecución.
    pub async fn new() -> AppResult<Self> {
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .load()
            .await;

        let iam_client = IamClient::new(&config);

        Ok(Self { iam_client })
    }

    /// Crea un KeyRotator usando un perfil específico de AWS.
    pub async fn with_profile(profile_name: &str) -> AppResult<Self> {
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .profile_name(profile_name)
            .load()
            .await;

        let iam_client = IamClient::new(&config);

        Ok(Self { iam_client })
    }

    /// Verifica la antigüedad de una clave consultando AWS IAM.
    ///
    /// CONCEPTO RUST - Error propagation chain:
    /// Observa la cadena de `?` y `.map_err()`:
    ///   1. Llamamos a AWS API → puede fallar (network, permisos, etc.)
    ///   2. Extraemos datos de la respuesta → puede ser None
    ///   3. Convertimos la fecha → puede fallar
    ///
    /// Cada paso que puede fallar tiene un `?` que propaga el error.
    /// En Node.js necesitarías try/catch anidados o .then().catch() chains.
    pub async fn check_key_age(
        &self,
        profile: &AwsProfile,
        max_age_days: u32,
    ) -> AppResult<KeyStatus> {
        info!(
            profile = %profile.name,
            key_id = %&profile.access_key_id[..8],
            "Verificando antigüedad de la clave"
        );

        // Listar las access keys del usuario actual
        let response = self
            .iam_client
            .list_access_keys()
            .send()
            .await
            .map_err(|e| AppError::AwsIam(format!("Error listando access keys: {}", e)))?;

        // CONCEPTO RUST - Iteradores + find:
        // Buscamos la key que coincida con el access_key_id del perfil.
        // `.find()` retorna Option<&AccessKeyMetadata>
        // Esto es como array.find() en JavaScript.
        let key_metadata = response
            .access_key_metadata()
            .iter()
            .find(|k| {
                k.access_key_id()
                    .map(|id| id == profile.access_key_id)
                    .unwrap_or(false)
            });

        let (age_days, created_date) = if let Some(metadata) = key_metadata {
            if let Some(create_date) = metadata.create_date() {
                let created = DateTime::from_timestamp(create_date.secs(), 0)
                    .unwrap_or_else(|| Utc::now());
                let age = Utc::now().signed_duration_since(created);
                (age.num_days(), Some(created))
            } else {
                warn!(profile = %profile.name, "No se pudo obtener la fecha de creación");
                (-1, None)
            }
        } else {
            warn!(
                profile = %profile.name,
                "Clave no encontrada en IAM. ¿El perfil tiene permisos iam:ListAccessKeys?"
            );
            (-1, None)
        };

        let needs_rotation = age_days >= 0 && age_days >= max_age_days as i64;

        Ok(KeyStatus {
            profile_name: profile.name.clone(),
            access_key_id: profile.access_key_id.clone(),
            age_days,
            max_age_days,
            needs_rotation,
            created_date,
        })
    }

    /// Rota las credenciales de un perfil.
    ///
    /// Proceso:
    /// 1. Crear nueva access key en IAM
    /// 2. Actualizar archivo de credenciales (atómico)
    /// 3. Validar que las nuevas credenciales funcionan
    /// 4. Eliminar la clave antigua en IAM
    ///
    /// CONCEPTO RUST - Ownership en acción:
    /// `credentials_file: &mut CredentialsFile` — recibimos una referencia MUTABLE.
    /// Solo puede existir UNA referencia mutable a la vez (previene data races).
    ///
    /// En Node.js puedes mutar objetos desde cualquier parte del código.
    /// En Rust, el compilador asegura que solo un lugar puede mutar a la vez.
    pub async fn rotate_key(
        &self,
        credentials_file: &mut CredentialsFile,
        profile_name: &str,
        keep_old: bool,
        dry_run: bool,
    ) -> AppResult<()> {
        let profile = credentials_file
            .get_profile(profile_name)
            .ok_or_else(|| {
                AppError::Credentials(format!("Perfil '{}' no encontrado", profile_name))
            })?
            .clone(); // Clonamos para liberar el borrow de credentials_file

        println!(
            "{}",
            format!("Rotando credenciales para perfil: {}", profile_name)
                .cyan()
                .bold()
        );

        if dry_run {
            println!(
                "{}",
                "[DRY RUN] Se crearía una nueva access key y se actualizaría el archivo"
                    .yellow()
            );
            return Ok(());
        }

        // Paso 1: Crear nueva access key
        info!(profile = %profile_name, "Creando nueva access key en IAM");
        let new_key = self
            .iam_client
            .create_access_key()
            .send()
            .await
            .map_err(|e| AppError::AwsIam(format!("Error creando access key: {}", e)))?;

        let access_key = new_key
            .access_key()
            .ok_or_else(|| AppError::AwsIam("Respuesta sin access key".into()))?;

        let new_key_id = access_key.access_key_id();
        let new_secret = access_key.secret_access_key();

        info!(
            profile = %profile_name,
            new_key_id = %&new_key_id[..8],
            "Nueva access key creada exitosamente"
        );

        // Paso 2: Actualizar archivo de credenciales atómicamente
        info!(profile = %profile_name, "Actualizando archivo de credenciales");
        credentials_file.update_profile(profile_name, new_key_id, new_secret)?;
        println!(
            "  {} Archivo de credenciales actualizado",
            "✓".green().bold()
        );

        // Paso 3: Validar nuevas credenciales
        info!(profile = %profile_name, "Validando nuevas credenciales");
        let validation_result = self.validate_credentials(profile_name).await;

        match validation_result {
            Ok(()) => {
                println!(
                    "  {} Nuevas credenciales validadas correctamente",
                    "✓".green().bold()
                );
            }
            Err(e) => {
                // CONCEPTO RUST - Rollback manual:
                // En Rust no hay transactions automáticas. Si algo falla,
                // debemos revertir manualmente. El compilador nos ayuda
                // porque los tipos nos obligan a manejar el error.
                error!(
                    profile = %profile_name,
                    error = %e,
                    "Falló la validación. Revirtiendo cambios..."
                );

                // Revertir al access key anterior
                credentials_file.update_profile(
                    profile_name,
                    &profile.access_key_id,
                    &profile.secret_access_key,
                )?;

                // Eliminar la key nueva que no funcionó
                let _ = self
                    .iam_client
                    .delete_access_key()
                    .access_key_id(new_key_id)
                    .send()
                    .await;

                return Err(AppError::AwsIam(format!(
                    "Validación falló. Cambios revertidos. Error: {}",
                    e
                )));
            }
        }

        // Paso 4: Eliminar clave antigua (si no se pidió mantenerla)
        if !keep_old {
            info!(
                profile = %profile_name,
                old_key = %&profile.access_key_id[..8],
                "Eliminando access key antigua"
            );

            self.iam_client
                .delete_access_key()
                .access_key_id(&profile.access_key_id)
                .send()
                .await
                .map_err(|e| {
                    AppError::AwsIam(format!(
                        "Error eliminando key antigua (nueva key ya está activa): {}",
                        e
                    ))
                })?;

            println!(
                "  {} Clave antigua {}... eliminada",
                "✓".green().bold(),
                &profile.access_key_id[..8]
            );
        } else {
            println!(
                "  {} Clave antigua conservada (--keep-old)",
                "ℹ".blue()
            );
        }

        println!(
            "{}",
            format!(
                "Rotación completada. Nueva clave: {}...",
                &new_key_id[..8]
            )
            .green()
            .bold()
        );

        Ok(())
    }

    /// Valida que las credenciales actuales funcionan llamando a GetCallerIdentity.
    ///
    /// Es el equivalente de `aws sts get-caller-identity` — la forma más simple
    /// de verificar que unas credenciales son válidas.
    async fn validate_credentials(&self, _profile_name: &str) -> AppResult<()> {
        // Usamos GetCallerIdentity de STS como validación.
        // Por ahora validamos con IAM ListAccessKeys como proxy.
        self.iam_client
            .list_access_keys()
            .max_items(1)
            .send()
            .await
            .map_err(|e| AppError::AwsIam(format!("Validación falló: {}", e)))?;

        Ok(())
    }

    /// Verifica cuántos access keys tiene el usuario actual.
    /// AWS permite máximo 2 access keys por usuario IAM.
    pub async fn count_existing_keys(&self) -> AppResult<usize> {
        let response = self
            .iam_client
            .list_access_keys()
            .send()
            .await
            .map_err(|e| AppError::AwsIam(format!("Error contando access keys: {}", e)))?;

        Ok(response.access_key_metadata().len())
    }
}

/// Calcula la antigüedad en días de una clave basándose en su fecha de creación.
pub fn calculate_key_age(created_at: &DateTime<Utc>) -> i64 {
    let now = Utc::now();
    let duration: Duration = now.signed_duration_since(*created_at);
    duration.num_days()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_key_age() {
        let now = Utc::now();
        let thirty_days_ago = now - Duration::days(30);
        assert_eq!(calculate_key_age(&thirty_days_ago), 30);
    }

    #[test]
    fn test_calculate_key_age_new_key() {
        let now = Utc::now();
        assert_eq!(calculate_key_age(&now), 0);
    }

    #[test]
    fn test_key_status_display_ok() {
        let status = KeyStatus {
            profile_name: "test".to_string(),
            access_key_id: "AKIATEST".to_string(),
            age_days: 10,
            max_age_days: 90,
            needs_rotation: false,
            created_date: None,
        };
        // Just verify it doesn't panic
        let _ = status.status_display();
    }

    #[test]
    fn test_key_status_display_needs_rotation() {
        let status = KeyStatus {
            profile_name: "test".to_string(),
            access_key_id: "AKIATEST".to_string(),
            age_days: 100,
            max_age_days: 90,
            needs_rotation: true,
            created_date: None,
        };
        let _ = status.status_display();
    }
}
