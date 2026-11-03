use crate::state::{AppState, MinecraftProfile};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Semaphore;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InstallProgress {
    pub step: String,
    pub percentage: f32,
    pub details: String,
}

// -----------------------------------------------------------------------------
// 1. Microsoft Device-Code OAuth2 Flow
// -----------------------------------------------------------------------------

#[tauri::command]
pub async fn login_microsoft(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_dir: String,
) -> Result<MinecraftProfile, String> {
    let client = reqwest::Client::new();
    let client_id = "00000000402b5328"; // Standard public client id for Xbox authentication

    // Step 1: Request Device Code
    let device_resp = client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode")
        .form(&[
            ("client_id", client_id),
            ("scope", "service::user.auth.xboxlive.com::MBI_SSL"),
        ])
        .send()
        .await
        .map_err(|e| format!("Failed to request device code: {e}"))?;

    let device_code_data: DeviceCodeResponse = device_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse device code response: {e}"))?;

    // Emit event so the UI can show the code and verification link
    let _ = app.emit("auth-device-code", &device_code_data);

    // Step 2: Poll token endpoint
    let interval = if device_code_data.interval == 0 { 5 } else { device_code_data.interval };
    let poll_url = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
    let start_time = tokio::time::Instant::now();
    let timeout = tokio::time::Duration::from_secs(device_code_data.expires_in);

    let ms_access_token: String = loop {
        if start_time.elapsed() > timeout {
            return Err("Microsoft authentication timed out waiting for authorization".into());
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;

        let token_resp = client
            .post(poll_url)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", client_id),
                ("device_code", &device_code_data.device_code),
            ])
            .send()
            .await;

        if let Ok(resp) = token_resp {
            let val: serde_json::Value = resp.json().await.unwrap_or_default();
            if let Some(tok) = val.get("access_token").and_then(|v| v.as_str()) {
                break tok.to_string();
            }

            if let Some(err) = val.get("error").and_then(|v| v.as_str()) {
                if err != "authorization_pending" && err != "slow_down" {
                    return Err(format!("Microsoft OAuth error: {err}"));
                }
            }
        }
    };

    // Step 3: Authenticate with Xbox Live
    let xbl_auth_payload = serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={}", ms_access_token)
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT"
    });

    let xbl_resp = client
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&xbl_auth_payload)
        .send()
        .await
        .map_err(|e| format!("XBL Authentication failed: {e}"))?;

    let xbl_data: serde_json::Value = xbl_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse XBL response: {e}"))?;

    let xbl_token = xbl_data["Token"]
        .as_str()
        .ok_or_else(|| "Missing XBL token in response".to_string())?;

    let user_hash = xbl_data["DisplayClaims"]["xui"][0]["uhs"]
        .as_str()
        .ok_or_else(|| "Missing XBL user hash".to_string())?;

    // Step 4: Obtain XSTS Token for Minecraft Services
    let xsts_payload = serde_json::json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_token]
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT"
    });

    let xsts_resp = client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&xsts_payload)
        .send()
        .await
        .map_err(|e| format!("XSTS Authorization failed: {e}"))?;

    let xsts_data: serde_json::Value = xsts_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse XSTS response: {e}"))?;

    let xsts_token = xsts_data["Token"]
        .as_str()
        .ok_or_else(|| "Missing XSTS token in response".to_string())?;

    // Step 5: Login to Minecraft Services
    let mc_auth_payload = serde_json::json!({
        "identityToken": format!("XBL3.0 x={};{}", user_hash, xsts_token)
    });

    let mc_auth_resp = client
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .header("Content-Type", "application/json")
        .json(&mc_auth_payload)
        .send()
        .await
        .map_err(|e| format!("Minecraft login failed: {e}"))?;

    let mc_auth_data: serde_json::Value = mc_auth_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Minecraft authentication response: {e}"))?;

    let mc_access_token = mc_auth_data["access_token"]
        .as_str()
        .ok_or_else(|| "Missing Minecraft access_token".to_string())?;

    // Step 6: Fetch Profile Details
    let profile_resp = client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(mc_access_token)
        .send()
        .await
        .map_err(|e| format!("Failed to query Minecraft profile: {e}"))?;

    let profile_data: serde_json::Value = profile_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Minecraft profile JSON: {e}"))?;

    let uuid = profile_data["id"]
        .as_str()
        .ok_or_else(|| "Missing Minecraft uuid in profile response".to_string())?
        .to_string();

    let username = profile_data["name"]
        .as_str()
        .ok_or_else(|| "Missing Minecraft username in profile response".to_string())?
        .to_string();

    let profile = MinecraftProfile {
        uuid,
        username,
        access_token: mc_access_token.to_string(),
    };

    // Save to AppState
    {
        let mut guard = state.lock().await;
        guard.auth_profile = Some(profile.clone());
    }

    // Persist to <instance_dir>/auth.json
    let instance_path = Path::new(&instance_dir);
    if let Err(e) = fs::create_dir_all(instance_path) {
        eprintln!("Warning: failed creating instance_dir for auth.json: {e}");
    }
    let auth_file_path = instance_path.join("auth.json");
    if let Ok(serialized) = serde_json::to_string_pretty(&profile) {
        let _ = fs::write(auth_file_path, serialized);
    }

    Ok(profile)
}

#[tauri::command]
pub async fn load_saved_auth(
    state: State<'_, AppState>,
    instance_dir: String,
) -> Result<Option<MinecraftProfile>, String> {
    let auth_file = Path::new(&instance_dir).join("auth.json");
    if auth_file.exists() {
        if let Ok(content) = fs::read_to_string(&auth_file) {
            if let Ok(profile) = serde_json::from_str::<MinecraftProfile>(&content) {
                let mut guard = state.lock().await;
                guard.auth_profile = Some(profile.clone());
                return Ok(Some(profile));
            }
        }
    }
    Ok(None)
}

// -----------------------------------------------------------------------------
// 2. Java 21 & Minecraft 1.21.1 + NeoForge Installer
// -----------------------------------------------------------------------------

pub fn resolve_java_path(app: &AppHandle) -> PathBuf {
    let app_data = app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
    let local_java = app_data.join("runtimes").join("java21").join("bin").join("java.exe");
    if local_java.exists() {
        local_java
    } else {
        PathBuf::from("java")
    }
}

pub async fn ensure_portable_java21(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data = app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
    let java_runtime_dir = app_data.join("runtimes").join("java21");
    let java_bin = java_runtime_dir.join("bin").join("java.exe");

    if java_bin.exists() {
        return Ok(java_bin);
    }

    let _ = app.emit(
        "install-progress",
        InstallProgress {
            step: "Java 21 Provisioning".into(),
            percentage: 5.0,
            details: "Querying Adoptium Temurin 21 runtime...".into(),
        },
    );

    let client = reqwest::Client::new();
    let adoptium_url = "https://api.adoptium.net/v3/binary/latest/21/ga/windows/x64/jdk/hotspot/normal/eclipse?project=jdk";

    let resp = client
        .get(adoptium_url)
        .send()
        .await
        .map_err(|e| format!("Failed to reach Adoptium API: {e}"))?;

    let total_bytes = resp.content_length().unwrap_or(200_000_000);
    let mut downloaded_bytes = 0u64;

    let temp_zip_path = app_data.join("temp_java21.zip");
    if let Some(parent) = temp_zip_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let mut temp_file = File::create(&temp_zip_path)
        .map_err(|e| format!("Failed creating temp file for Java 21: {e}"))?;

    let mut stream = resp.bytes_stream();
    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.map_err(|e| format!("Failed streaming Java archive: {e}"))?;
        temp_file
            .write_all(&chunk)
            .map_err(|e| format!("Failed writing Java chunk: {e}"))?;
        downloaded_bytes += chunk.len() as u64;

        let pct = 5.0 + (downloaded_bytes as f32 / total_bytes as f32) * 20.0;
        let _ = app.emit(
            "install-progress",
            InstallProgress {
                step: "Downloading Java 21".into(),
                percentage: pct,
                details: format!(
                    "{:.1} MB / {:.1} MB",
                    downloaded_bytes as f64 / 1_048_576.0,
                    total_bytes as f64 / 1_048_576.0
                ),
            },
        );
    }
    drop(temp_file);

    let _ = app.emit(
        "install-progress",
        InstallProgress {
            step: "Extracting Java 21".into(),
            percentage: 26.0,
            details: "Extracting JDK archive...".into(),
        },
    );

    // Unpack ZIP
    let zip_file = File::open(&temp_zip_path)
        .map_err(|e| format!("Failed opening downloaded Java zip: {e}"))?;
    let mut archive = zip::ZipArchive::new(BufReader::new(zip_file))
        .map_err(|e| format!("Invalid Java ZIP archive: {e}"))?;

    let _ = fs::create_dir_all(&java_runtime_dir);

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let entry_name = entry.mangled_name();
        
        // Strip top-level directory (e.g. jdk-21.0.x+y/)
        let mut components = entry_name.components();
        components.next(); // Skip root
        let relative_path = components.as_path();
        if relative_path.as_os_str().is_empty() {
            continue;
        }

        let target_path = java_runtime_dir.join(relative_path);
        if entry.is_dir() {
            let _ = fs::create_dir_all(&target_path);
        } else {
            if let Some(p) = target_path.parent() {
                let _ = fs::create_dir_all(p);
            }
            let mut outfile = File::create(&target_path).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut outfile).map_err(|e| e.to_string())?;
        }
    }

    let _ = fs::remove_file(temp_zip_path);

    if java_bin.exists() {
        Ok(java_bin)
    } else {
        Err("Failed to locate java.exe after extracting Adoptium JDK".into())
    }
}

// -----------------------------------------------------------------------------
// Mojang 1.21.1 Assets & Libraries Downloader
// -----------------------------------------------------------------------------

async fn download_minecraft_client_assets(
    app: &AppHandle,
    client: &reqwest::Client,
    instance_dir: &Path,
) -> Result<(), String> {
    let mc_dir = instance_dir.join(".minecraft");
    let versions_dir = mc_dir.join("versions").join("1.21.1");
    let libraries_dir = mc_dir.join("libraries");
    let assets_dir = mc_dir.join("assets");
    let _ = fs::create_dir_all(&versions_dir);
    let _ = fs::create_dir_all(&libraries_dir);
    let _ = fs::create_dir_all(assets_dir.join("indexes"));
    let _ = fs::create_dir_all(assets_dir.join("objects"));

    let _ = app.emit(
        "install-progress",
        InstallProgress {
            step: "Game Provisioning".into(),
            percentage: 30.0,
            details: "Fetching Mojang version manifest...".into(),
        },
    );

    // Fetch Mojang Version Manifest
    let manifest_val: serde_json::Value = client
        .get("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")
        .send()
        .await
        .map_err(|e| format!("Failed reaching Mojang manifest: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Failed parsing Mojang manifest: {e}"))?;

    let version_1211_entry = manifest_val["versions"]
        .as_array()
        .and_then(|arr| arr.iter().find(|v| v["id"].as_str() == Some("1.21.1")))
        .ok_or_else(|| "1.21.1 entry not found in Mojang manifest".to_string())?;

    let version_json_url = version_1211_entry["url"]
        .as_str()
        .ok_or_else(|| "Missing 1.21.1 version json url".to_string())?;

    // Download 1.21.1.json
    let version_json_bytes = client
        .get(version_json_url)
        .send()
        .await
        .map_err(|e| format!("Failed downloading 1.21.1.json: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("Failed reading 1.21.1.json bytes: {e}"))?;

    let version_json_path = versions_dir.join("1.21.1.json");
    fs::write(&version_json_path, &version_json_bytes)
        .map_err(|e| format!("Failed saving 1.21.1.json: {e}"))?;

    let version_data: serde_json::Value = serde_json::from_slice(&version_json_bytes)
        .map_err(|e| format!("Failed parsing 1.21.1 json: {e}"))?;

    // Download Client JAR
    let _ = app.emit(
        "install-progress",
        InstallProgress {
            step: "Game Provisioning".into(),
            percentage: 35.0,
            details: "Downloading Minecraft 1.21.1 client JAR...".into(),
        },
    );

    if let Some(client_url) = version_data["downloads"]["client"]["url"].as_str() {
        let client_jar_path = versions_dir.join("1.21.1.jar");
        if !client_jar_path.exists() {
            let jar_bytes = client
                .get(client_url)
                .send()
                .await
                .map_err(|e| format!("Failed to download client jar: {e}"))?
                .bytes()
                .await
                .map_err(|e| format!("Failed to read client jar: {e}"))?;
            fs::write(client_jar_path, jar_bytes)
                .map_err(|e| format!("Failed to write client jar: {e}"))?;
        }
    }

    // Download Libraries
    if let Some(libs) = version_data["libraries"].as_array() {
        for lib in libs {
            if let Some(artifact) = lib.get("downloads").and_then(|d| d.get("artifact")) {
                if let (Some(path_str), Some(url)) = (
                    artifact.get("path").and_then(|p| p.as_str()),
                    artifact.get("url").and_then(|u| u.as_str()),
                ) {
                    let dest = libraries_dir.join(path_str);
                    if !dest.exists() {
                        if let Some(parent) = dest.parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        if let Ok(resp) = client.get(url).send().await {
                            if let Ok(bytes) = resp.bytes().await {
                                let _ = fs::write(dest, bytes);
                            }
                        }
                    }
                }
            }
        }
    }

    // Download Asset Index & Assets
    if let Some(asset_index) = version_data.get("assetIndex") {
        if let (Some(idx_id), Some(idx_url)) = (
            asset_index.get("id").and_then(|i| i.as_str()),
            asset_index.get("url").and_then(|u| u.as_str()),
        ) {
            let index_path = assets_dir.join("indexes").join(format!("{}.json", idx_id));
            let index_bytes = client
                .get(idx_url)
                .send()
                .await
                .map_err(|e| format!("Failed fetching asset index: {e}"))?
                .bytes()
                .await
                .map_err(|e| format!("Failed reading asset index: {e}"))?;

            fs::write(&index_path, &index_bytes)
                .map_err(|e| format!("Failed saving asset index: {e}"))?;

            if let Ok(index_json) = serde_json::from_slice::<serde_json::Value>(&index_bytes) {
                if let Some(objects_map) = index_json.get("objects").and_then(|o| o.as_object()) {
                    let semaphore = Arc::new(Semaphore::new(16));
                    let mut tasks = Vec::new();

                    for (_name, obj) in objects_map.iter().take(500) {
                        if let Some(hash) = obj.get("hash").and_then(|h| h.as_str()) {
                            if hash.len() >= 2 {
                                let prefix = &hash[0..2];
                                let obj_dir = assets_dir.join("objects").join(prefix);
                                let obj_file = obj_dir.join(hash);

                                if !obj_file.exists() {
                                    let sem = semaphore.clone();
                                    let cl = client.clone();
                                    let url = format!(
                                        "https://resources.download.minecraft.net/{}/{}",
                                        prefix, hash
                                    );

                                    tasks.push(tokio::spawn(async move {
                                        let _permit = sem.acquire().await;
                                        let _ = fs::create_dir_all(&obj_dir);
                                        if let Ok(resp) = cl.get(&url).send().await {
                                            if let Ok(bytes) = resp.bytes().await {
                                                let _ = fs::write(&obj_file, bytes);
                                            }
                                        }
                                    }));
                                }
                            }
                        }
                    }

                    for task in tasks {
                        let _ = task.await;
                    }
                }
            }
        }
    }

    Ok(())
}

// -----------------------------------------------------------------------------
// NeoForge 1.21.1 Headless Provisioner
// -----------------------------------------------------------------------------

#[tauri::command]
pub async fn install_instance(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_dir: String,
) -> Result<(), String> {
    let instance_path = PathBuf::from(&instance_dir);
    fs::create_dir_all(&instance_path)
        .map_err(|e| format!("Failed creating instance directory: {e}"))?;

    // 1. Ensure Java 21
    let java_bin = ensure_portable_java21(&app).await?;

    let client = reqwest::Client::new();

    // 2. Download vanilla client & assets
    download_minecraft_client_assets(&app, &client, &instance_path).await?;

    // 3. Download NeoForge 1.21.1 Installer
    let _ = app.emit(
        "install-progress",
        InstallProgress {
            step: "NeoForge Provisioning".into(),
            percentage: 60.0,
            details: "Resolving NeoForge 1.21.1 installer...".into(),
        },
    );

    let neoforge_version = "21.1.187"; // Stable 1.21.1 NeoForge build
    let installer_url = format!(
        "https://maven.neoforged.net/releases/net/neoforged/neoforge/{version}/neoforge-{version}-installer.jar",
        version = neoforge_version
    );

    let installer_path = instance_path.join(format!("neoforge-{}-installer.jar", neoforge_version));
    if !installer_path.exists() {
        let installer_bytes = client
            .get(&installer_url)
            .send()
            .await
            .map_err(|e| format!("Failed to download NeoForge installer: {e}"))?
            .bytes()
            .await
            .map_err(|e| format!("Failed reading NeoForge installer: {e}"))?;
        fs::write(&installer_path, installer_bytes)
            .map_err(|e| format!("Failed to save NeoForge installer: {e}"))?;
    }

    // 4. Run --installClient
    let _ = app.emit(
        "install-progress",
        InstallProgress {
            step: "NeoForge Provisioning".into(),
            percentage: 75.0,
            details: "Running headless NeoForge client installation...".into(),
        },
    );

    let client_target_dir = instance_path.join(".minecraft");
    let mut client_install_cmd = tokio::process::Command::new(&java_bin);
    client_install_cmd
        .arg("-jar")
        .arg(&installer_path)
        .arg("--installClient")
        .arg(&client_target_dir);

    let client_output = client_install_cmd
        .output()
        .await
        .map_err(|e| format!("Failed running NeoForge client installer: {e}"))?;

    if !client_output.status.success() {
        let err_text = String::from_utf8_lossy(&client_output.stderr);
        eprintln!("NeoForge client installer warning/error: {err_text}");
    }

    // 5. Run --installServer
    let _ = app.emit(
        "install-progress",
        InstallProgress {
            step: "NeoForge Provisioning".into(),
            percentage: 85.0,
            details: "Running headless NeoForge dedicated server installation...".into(),
        },
    );

    let server_target_dir = instance_path.join("server");
    fs::create_dir_all(&server_target_dir)
        .map_err(|e| format!("Failed creating server directory: {e}"))?;

    let mut server_install_cmd = tokio::process::Command::new(&java_bin);
    server_install_cmd
        .arg("-jar")
        .arg(&installer_path)
        .arg("--installServer")
        .arg(&server_target_dir);

    let server_output = server_install_cmd
        .output()
        .await
        .map_err(|e| format!("Failed running NeoForge server installer: {e}"))?;

    if !server_output.status.success() {
        let err_text = String::from_utf8_lossy(&server_output.stderr);
        eprintln!("NeoForge server installer note: {err_text}");
    }

    // 6. Write eula.txt
    let eula_file = server_target_dir.join("eula.txt");
    fs::write(eula_file, "eula=true\n")
        .map_err(|e| format!("Failed writing eula.txt: {e}"))?;

    // 7. Write server.properties
    let props_file = server_target_dir.join("server.properties");
    let properties_content = "\
level-name=../.minecraft/saves/World1\n\
online-mode=true\n\
simulation-distance=6\n\
view-distance=10\n\
sync-chunk-writes=false\n\
server-port=25565\n\
enable-rcon=false\n";

    fs::write(props_file, properties_content)
        .map_err(|e| format!("Failed writing server.properties: {e}"))?;

    let _ = app.emit(
        "install-progress",
        InstallProgress {
            step: "Complete".into(),
            percentage: 100.0,
            details: "NeoForge 1.21.1 game and server ready!".into(),
        },
    );

    let _ = state;
    Ok(())
}
