use crate::state::{AppState, MinecraftProfile};
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VersionMetaItem {
    pub id: String,
    pub release_type: String,
    pub release_time: String,
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

    let device_resp = client
        .post("https://login.live.com/oauth20_connect.srf")
        .form(&[
            ("client_id", client_id),
            ("scope", "service::user.auth.xboxlive.com::MBI_SSL"),
            ("response_type", "device_code"),
        ])
        .send()
        .await
        .map_err(|e| format!("Failed to request device code: {e}"))?;

    let device_code_data: DeviceCodeResponse = device_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse device code response: {e}"))?;

    let _ = app.emit("auth-device-code", &device_code_data);

    let interval = if device_code_data.interval == 0 { 5 } else { device_code_data.interval };
    let poll_url = "https://login.live.com/oauth20_token.srf";
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
    let userhash = xbl_data["DisplayClaims"]["xui"][0]["uhs"]
        .as_str()
        .ok_or_else(|| "Missing userhash in XBL response".to_string())?;

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

    let mc_login_payload = serde_json::json!({
        "identityToken": format!("XBL3.0 x={};{}", userhash, xsts_token)
    });

    let mc_login_resp = client
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&mc_login_payload)
        .send()
        .await
        .map_err(|e| format!("Minecraft login failed: {e}"))?;

    let mc_login_data: serde_json::Value = mc_login_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Minecraft login response: {e}"))?;

    let mc_access_token = mc_login_data["access_token"]
        .as_str()
        .ok_or_else(|| "Missing access_token in Minecraft login response".to_string())?;

    let profile_resp = client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(mc_access_token)
        .send()
        .await
        .map_err(|e| format!("Failed to query Minecraft profile: {e}"))?;

    let profile_data: serde_json::Value = profile_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Minecraft profile: {e}"))?;

    let uuid = profile_data["id"]
        .as_str()
        .ok_or_else(|| "Missing Minecraft UUID in profile response".to_string())?
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

    {
        let mut guard = state.lock().await;
        guard.auth_profile = Some(profile.clone());
    }

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
// 2. Dynamic Version Metadata Service (Official Mojang Manifest)
// -----------------------------------------------------------------------------

#[tauri::command]
pub async fn list_all_minecraft_versions() -> Result<Vec<VersionMetaItem>, String> {
    let client = reqwest::Client::builder()
        .user_agent("Conduit/1.0.0 (https://conduit.gg)")
        .build()
        .map_err(|e| e.to_string())?;

    let manifest_val: serde_json::Value = client
        .get("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")
        .send()
        .await
        .map_err(|e| format!("Failed reaching Mojang manifest: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Failed parsing Mojang manifest: {e}"))?;

    let mut versions = Vec::new();
    if let Some(arr) = manifest_val["versions"].as_array() {
        for v in arr {
            if let Some(id) = v["id"].as_str() {
                let release_type = v["type"].as_str().unwrap_or("release").to_string();
                let release_time = v["releaseTime"].as_str().unwrap_or("").to_string();
                versions.push(VersionMetaItem {
                    id: id.to_string(),
                    release_type,
                    release_time,
                });
            }
        }
    }

    Ok(versions)
}

// -----------------------------------------------------------------------------
// 3. Java 21 & Runtimes Resolver
// -----------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaInstallation {
    pub name: String,
    pub path: String,
    pub version: String,
    pub is_recommended: bool,
}

#[tauri::command]
pub async fn detect_installed_javas() -> Result<Vec<JavaInstallation>, String> {
    let mut javas = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    // 1. Check SquidServers java-paths.json if present
    if let Ok(appdata) = std::env::var("APPDATA") {
        let sq_java_json = PathBuf::from(&appdata).join("squidservers").join("java-paths.json");
        if sq_java_json.exists() {
            if let Ok(content) = fs::read_to_string(&sq_java_json) {
                if let Ok(json_arr) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(arr) = json_arr.as_array() {
                        for item in arr {
                            if let Some(p_str) = item.get("path").and_then(|v| v.as_str()) {
                                let p = PathBuf::from(p_str);
                                if p.exists() && seen_paths.insert(p_str.to_lowercase()) {
                                    let ver = item.get("version").and_then(|v| v.as_str()).unwrap_or("21.0.0").to_string();
                                    let is_squid = item.get("isSquidJava").and_then(|v| v.as_bool()).unwrap_or(false);
                                    let name = if is_squid {
                                        format!("Java {} (SquidServers JDK 21)", ver)
                                    } else if p_str.contains(".gradle") {
                                        format!("Java {} (Adoptium JDK 21)", ver)
                                    } else if p_str.contains("Android Studio") {
                                        format!("Java {} (Android Studio JBR)", ver)
                                    } else if p_str.contains("WebStorm") || p_str.contains("IntelliJ") {
                                        format!("Java {} (JetBrains JBR)", ver)
                                    } else {
                                        format!("Java {} ({})", ver, p.parent().and_then(|p| p.parent()).and_then(|p| p.file_name()).map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| "Runtime".to_string()))
                                    };
                                    let is_rec = ver.starts_with("21");
                                    javas.push(JavaInstallation {
                                        name,
                                        path: p_str.to_string(),
                                        version: ver,
                                        is_recommended: is_rec,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Check well-known system locations & user environment directories
    let mut candidates = vec![
        ("Java 21 (Adoptium Temurin)", "C:\\Program Files\\Eclipse Adoptium\\jdk-21.0.0-hotspot\\bin\\java.exe".to_string(), "21.0.0", true),
        ("Java 21 (Eclipse Adoptium)", "C:\\Program Files\\Eclipse Adoptium\\jdk-21\\bin\\java.exe".to_string(), "21.0.0", true),
        ("Java 21 (Oracle JDK)", "C:\\Program Files\\Java\\jdk-21\\bin\\java.exe".to_string(), "21.0.0", true),
        ("Java 21 (Microsoft OpenJDK)", "C:\\Program Files\\Microsoft\\jdk-21\\bin\\java.exe".to_string(), "21.0.0", true),
        ("Java 25 (Android Studio JBR)", "C:\\Program Files\\Android\\Android Studio\\jbr\\bin\\java.exe".to_string(), "25.0.2", false),
    ];

    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        let p_up = PathBuf::from(&userprofile);
        candidates.push((
            "Java 21 (Adoptium Gradle)",
            p_up.join(".gradle\\jdks\\eclipse_adoptium-21-amd64-windows.2\\bin\\java.exe").to_string_lossy().to_string(),
            "21.0.7",
            true,
        ));
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        let p_ad = PathBuf::from(&appdata);
        candidates.push((
            "Java 21 (SquidServers JDK)",
            p_ad.join("squidservers\\java\\jdk-21\\bin\\java.exe").to_string_lossy().to_string(),
            "21.0.12",
            true,
        ));
    }

    for (name, path_str, ver, rec) in candidates {
        let p = PathBuf::from(&path_str);
        if p.exists() && seen_paths.insert(path_str.to_lowercase()) {
            javas.push(JavaInstallation {
                name: name.to_string(),
                path: path_str,
                version: ver.to_string(),
                is_recommended: rec,
            });
        }
    }

    // Sort: recommended first, then by version desc
    javas.sort_by(|a, b| b.is_recommended.cmp(&a.is_recommended));
    Ok(javas)
}

pub fn resolve_java_path(app: &AppHandle) -> PathBuf {
    let app_data = app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
    let local_java = app_data.join("runtimes").join("java21").join("bin").join("java.exe");
    if local_java.exists() {
        return local_java;
    }

    let mut candidates = Vec::new();

    if let Ok(appdata) = std::env::var("APPDATA") {
        candidates.push(PathBuf::from(&appdata).join("squidservers").join("java").join("jdk-21").join("bin").join("java.exe"));
    }

    candidates.push(PathBuf::from("C:\\Program Files\\Eclipse Adoptium\\jdk-21.0.0-hotspot\\bin\\java.exe"));
    candidates.push(PathBuf::from("C:\\Program Files\\Eclipse Adoptium\\jdk-21\\bin\\java.exe"));
    candidates.push(PathBuf::from("C:\\Program Files\\Java\\jdk-21\\bin\\java.exe"));
    candidates.push(PathBuf::from("C:\\Program Files\\Microsoft\\jdk-21\\bin\\java.exe"));

    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        candidates.push(PathBuf::from(&userprofile).join(".gradle\\jdks\\eclipse_adoptium-21-amd64-windows.2\\bin\\java.exe"));
    }

    for p in candidates {
        if p.exists() {
            return p;
        }
    }

    PathBuf::from("java")
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

    let client = reqwest::Client::builder()
        .user_agent("Conduit/1.0.0 (https://conduit.gg)")
        .build()
        .map_err(|e| e.to_string())?;

    let adoptium_url = "https://api.adoptium.net/v3/binary/latest/21/ga/windows/x64/jdk/hotspot/normal/eclipse?project=jdk";

    let resp = client
        .get(adoptium_url)
        .send()
        .await
        .map_err(|e| format!("Failed reaching Adoptium API: {e}"))?;

    let total_size = resp.content_length().unwrap_or(180_000_000);

    let temp_zip_path = app_data.join("temp_java21.zip");
    let mut file = File::create(&temp_zip_path)
        .map_err(|e| format!("Failed creating temp zip file: {e}"))?;

    let mut downloaded: u64 = 0;
    let mut stream = resp.bytes_stream();
    use futures_util::StreamExt;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("Error downloading Java 21 chunk: {e}"))?;
        file.write_all(&chunk)
            .map_err(|e| format!("Error writing Java 21 chunk: {e}"))?;
        downloaded += chunk.len() as u64;

        let pct = 5.0 + ((downloaded as f32 / total_size as f32) * 20.0);
        let _ = app.emit(
            "install-progress",
            InstallProgress {
                step: "Java 21 Provisioning".into(),
                percentage: pct,
                details: format!(
                    "Downloading Adoptium Temurin 21 ({:.1} MB / {:.1} MB)...",
                    downloaded as f32 / 1048576.0,
                    total_size as f32 / 1048576.0
                ),
            },
        );
    }

    let _ = app.emit(
        "install-progress",
        InstallProgress {
            step: "Java 21 Provisioning".into(),
            percentage: 26.0,
            details: "Extracting portable JDK archive...".into(),
        },
    );

    let zip_file = File::open(&temp_zip_path)
        .map_err(|e| format!("Failed opening downloaded Java zip: {e}"))?;
    let mut archive = zip::ZipArchive::new(BufReader::new(zip_file))
        .map_err(|e| format!("Invalid Java ZIP archive: {e}"))?;

    let _ = fs::create_dir_all(&java_runtime_dir);

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let entry_name = entry.mangled_name();
        
        let mut components = entry_name.components();
        components.next(); // Skip root folder
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
// 4. Vanilla Client & Assets Provisioner for Any Version
// -----------------------------------------------------------------------------

async fn download_minecraft_client_assets(
    app: &AppHandle,
    client: &reqwest::Client,
    instance_dir: &Path,
    game_version: &str,
) -> Result<(), String> {
    let mc_dir = instance_dir.join(".minecraft");
    let versions_dir = mc_dir.join("versions").join(game_version);
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
            details: format!("Querying Mojang manifest for Minecraft {game_version}..."),
        },
    );

    let manifest_val: serde_json::Value = client
        .get("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")
        .send()
        .await
        .map_err(|e| format!("Failed reaching Mojang manifest: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Failed parsing Mojang manifest: {e}"))?;

    let version_entry = manifest_val["versions"]
        .as_array()
        .and_then(|arr| arr.iter().find(|v| v["id"].as_str() == Some(game_version)))
        .ok_or_else(|| format!("{game_version} entry not found in Mojang manifest"))?;

    let version_json_url = version_entry["url"]
        .as_str()
        .ok_or_else(|| format!("Missing {game_version} version json url"))?;

    let version_json_bytes = client
        .get(version_json_url)
        .send()
        .await
        .map_err(|e| format!("Failed downloading {game_version}.json: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("Failed reading {game_version}.json bytes: {e}"))?;

    let version_json_path = versions_dir.join(format!("{}.json", game_version));
    fs::write(&version_json_path, &version_json_bytes)
        .map_err(|e| format!("Failed saving {game_version}.json: {e}"))?;

    let version_data: serde_json::Value = serde_json::from_slice(&version_json_bytes)
        .map_err(|e| format!("Failed parsing {game_version} json: {e}"))?;

    // Download Client JAR
    let _ = app.emit(
        "install-progress",
        InstallProgress {
            step: "Game Provisioning".into(),
            percentage: 35.0,
            details: format!("Downloading Minecraft {game_version} client JAR..."),
        },
    );

    if let Some(client_url) = version_data["downloads"]["client"]["url"].as_str() {
        let client_jar_path = versions_dir.join(format!("{}.jar", game_version));
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

    // Download vanilla server JAR if available (supports Vanilla server loader!)
    if let Some(server_url) = version_data["downloads"]["server"]["url"].as_str() {
        let server_dir = instance_dir.join("server");
        let _ = fs::create_dir_all(&server_dir);
        let server_jar_path = server_dir.join("server.jar");
        if !server_jar_path.exists() {
            if let Ok(resp) = client.get(server_url).send().await {
                if let Ok(bytes) = resp.bytes().await {
                    let _ = fs::write(server_jar_path, bytes);
                }
            }
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

                    for (_name, obj) in objects_map.iter() {
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
// 5. Universal Loader Provisioners (NeoForge, Fabric, Quilt, Paper, Purpur, Vanilla)
// -----------------------------------------------------------------------------

async fn install_paper(
    app: &AppHandle,
    client: &reqwest::Client,
    server_dir: &Path,
    game_version: &str,
) -> Result<(), String> {
    let _ = app.emit(
        "install-progress",
        InstallProgress {
            step: "PaperMC Provisioning".into(),
            percentage: 65.0,
            details: format!("Querying Fill v3 API for Paper {game_version}..."),
        },
    );

    let builds_url = format!("https://fill.papermc.io/v3/projects/paper/versions/{game_version}/builds");
    let builds_resp = client
        .get(&builds_url)
        .send()
        .await
        .map_err(|e| format!("Failed querying Paper builds: {e}"))?;

    let builds: Vec<serde_json::Value> = builds_resp
        .json()
        .await
        .map_err(|e| format!("Failed parsing Paper builds response: {e}"))?;

    let latest_build = builds.first().ok_or_else(|| format!("No Paper builds found for {game_version}"))?;
    let download_url = latest_build["downloads"]["server:default"]["url"]
        .as_str()
        .ok_or_else(|| "Missing server download URL in Paper build".to_string())?;

    let _ = app.emit(
        "install-progress",
        InstallProgress {
            step: "PaperMC Provisioning".into(),
            percentage: 75.0,
            details: format!("Downloading high-performance Paper dedicated server JAR..."),
        },
    );

    let jar_bytes = client
        .get(download_url)
        .send()
        .await
        .map_err(|e| format!("Failed downloading Paper server jar: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("Failed reading Paper server jar: {e}"))?;

    fs::write(server_dir.join("server.jar"), jar_bytes)
        .map_err(|e| format!("Failed saving server.jar: {e}"))?;

    Ok(())
}

async fn install_purpur(
    app: &AppHandle,
    client: &reqwest::Client,
    server_dir: &Path,
    game_version: &str,
) -> Result<(), String> {
    let _ = app.emit(
        "install-progress",
        InstallProgress {
            step: "Purpur Provisioning".into(),
            percentage: 65.0,
            details: format!("Resolving latest Purpur build for {game_version}..."),
        },
    );

    let version_info_url = format!("https://api.purpurmc.org/v2/purpur/{game_version}");
    let ver_resp = client
        .get(&version_info_url)
        .send()
        .await
        .map_err(|e| format!("Failed reaching Purpur API: {e}"))?;

    let ver_data: serde_json::Value = ver_resp
        .json()
        .await
        .map_err(|e| format!("Failed parsing Purpur metadata: {e}"))?;

    let latest_build = ver_data["builds"]["latest"]
        .as_str()
        .ok_or_else(|| format!("No Purpur builds found for {game_version}"))?;

    let download_url = format!("https://api.purpurmc.org/v2/purpur/{game_version}/{latest_build}/download");
    let jar_bytes = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| format!("Failed downloading Purpur jar: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("Failed reading Purpur jar: {e}"))?;

    fs::write(server_dir.join("server.jar"), jar_bytes)
        .map_err(|e| format!("Failed saving server.jar: {e}"))?;

    Ok(())
}

async fn install_fabric(
    app: &AppHandle,
    client: &reqwest::Client,
    java_bin: &Path,
    instance_path: &Path,
    game_version: &str,
) -> Result<(), String> {
    let _ = app.emit(
        "install-progress",
        InstallProgress {
            step: "Fabric Provisioning".into(),
            percentage: 60.0,
            details: format!("Downloading official Fabric installer..."),
        },
    );

    let installer_url = "https://maven.fabricmc.net/net/fabricmc/fabric-installer/1.1.2/fabric-installer-1.1.2.jar";
    let installer_path = instance_path.join("fabric-installer.jar");
    if !installer_path.exists() {
        let bytes = client
            .get(installer_url)
            .send()
            .await
            .map_err(|e| format!("Failed downloading Fabric installer: {e}"))?
            .bytes()
            .await
            .map_err(|e| format!("Failed reading Fabric installer: {e}"))?;
        fs::write(&installer_path, bytes)
            .map_err(|e| format!("Failed writing Fabric installer: {e}"))?;
    }

    // Install Client
    let client_target = instance_path.join(".minecraft");
    let mut cli_cmd = tokio::process::Command::new(java_bin);
    cli_cmd.arg("-jar").arg(&installer_path).arg("client").arg("-mcversion").arg(game_version).arg("-dir").arg(&client_target);
    #[cfg(target_os = "windows")]
    {
        cli_cmd.creation_flags(0x08000000);
    }
    let _ = cli_cmd.output().await;

    // Install Server
    let server_target = instance_path.join("server");
    let mut srv_cmd = tokio::process::Command::new(java_bin);
    srv_cmd.arg("-jar").arg(&installer_path).arg("server").arg("-mcversion").arg(game_version).arg("-dir").arg(&server_target).arg("-downloadMinecraft");
    #[cfg(target_os = "windows")]
    {
        srv_cmd.creation_flags(0x08000000);
    }
    let _ = srv_cmd.output().await;

    Ok(())
}

async fn install_quilt(
    app: &AppHandle,
    client: &reqwest::Client,
    java_bin: &Path,
    instance_path: &Path,
    game_version: &str,
) -> Result<(), String> {
    let _ = app.emit(
        "install-progress",
        InstallProgress {
            step: "Quilt Provisioning".into(),
            percentage: 60.0,
            details: format!("Downloading official Quilt installer..."),
        },
    );

    let installer_url = "https://maven.quiltmc.org/repository/release/org/quiltmc/quilt-installer/0.15.1/quilt-installer-0.15.1.jar";
    let installer_path = instance_path.join("quilt-installer.jar");
    if !installer_path.exists() {
        let bytes = client
            .get(installer_url)
            .send()
            .await
            .map_err(|e| format!("Failed downloading Quilt installer: {e}"))?
            .bytes()
            .await
            .map_err(|e| format!("Failed reading Quilt installer: {e}"))?;
        fs::write(&installer_path, bytes)
            .map_err(|e| format!("Failed writing Quilt installer: {e}"))?;
    }

    // Install Client
    let client_target = instance_path.join(".minecraft");
    let mut cli_cmd = tokio::process::Command::new(java_bin);
    cli_cmd.arg("-jar").arg(&installer_path).arg("install").arg("client").arg(game_version).arg("--install-dir").arg(&client_target);
    #[cfg(target_os = "windows")]
    {
        cli_cmd.creation_flags(0x08000000);
    }
    let _ = cli_cmd.output().await;

    // Install Server
    let server_target = instance_path.join("server");
    let mut srv_cmd = tokio::process::Command::new(java_bin);
    srv_cmd.arg("-jar").arg(&installer_path).arg("install").arg("server").arg(game_version).arg("--install-dir").arg(&server_target).arg("--download-server");
    #[cfg(target_os = "windows")]
    {
        srv_cmd.creation_flags(0x08000000);
    }
    let _ = srv_cmd.output().await;

    Ok(())
}

async fn install_neoforge(
    app: &AppHandle,
    client: &reqwest::Client,
    java_bin: &Path,
    instance_path: &Path,
    game_version: &str,
) -> Result<(), String> {
    let _ = app.emit(
        "install-progress",
        InstallProgress {
            step: "NeoForge Provisioning".into(),
            percentage: 60.0,
            details: format!("Resolving NeoForge installer for {game_version}..."),
        },
    );

    let neoforge_version = if game_version == "1.21.1" {
        "21.1.187".to_string()
    } else {
        format!("{}.0", game_version.replace("1.", ""))
    };

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

    // Run --installClient
    let client_target_dir = instance_path.join(".minecraft");
    let mut client_install_cmd = tokio::process::Command::new(java_bin);
    client_install_cmd
        .arg("-jar")
        .arg(&installer_path)
        .arg("--installClient")
        .arg(&client_target_dir);

    #[cfg(target_os = "windows")]
    {
        client_install_cmd.creation_flags(0x08000000);
    }

    let _ = client_install_cmd.output().await;

    // Run --installServer
    let server_target_dir = instance_path.join("server");
    let mut server_install_cmd = tokio::process::Command::new(java_bin);
    server_install_cmd
        .arg("-jar")
        .arg(&installer_path)
        .arg("--installServer")
        .arg(&server_target_dir);

    #[cfg(target_os = "windows")]
    {
        server_install_cmd.creation_flags(0x08000000);
    }

    let _ = server_install_cmd.output().await;

    Ok(())
}

// -----------------------------------------------------------------------------
// 6. Master Installation Orchestrator (Any Loader, Any Version)
// -----------------------------------------------------------------------------

#[tauri::command]
pub async fn install_instance(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_dir: String,
    loader: Option<String>,
    version: Option<String>,
) -> Result<(), String> {
    let chosen_loader = loader.unwrap_or_else(|| "NeoForge".into());
    let chosen_version = version.unwrap_or_else(|| "1.21.1".into());

    let instance_path = PathBuf::from(&instance_dir);
    fs::create_dir_all(&instance_path)
        .map_err(|e| format!("Failed creating instance directory: {e}"))?;

    // 1. Ensure Portable Java 21 Runtime
    let java_bin = ensure_portable_java21(&app).await?;

    let client = reqwest::Client::builder()
        .user_agent("Conduit/1.0.0 (https://conduit.gg)")
        .build()
        .map_err(|e| e.to_string())?;

    // 2. Download vanilla client & assets for the specified version
    download_minecraft_client_assets(&app, &client, &instance_path, &chosen_version).await?;

    // 3. Ensure launcher_profiles.json exists
    let launcher_profiles_path = instance_path.join(".minecraft").join("launcher_profiles.json");
    if !launcher_profiles_path.exists() {
        let dummy_profiles = serde_json::json!({
            "profiles": {},
            "settings": {
                "crashAssistance": true,
                "enableAdvanced": false,
                "enableAnalytics": false,
                "enableHistorical": false,
                "enableReleases": true,
                "enableSnapshots": false,
                "keepLauncherOpen": false,
                "profileSorting": "byLastPlayed",
                "showGameLog": false,
                "showMenu": false,
                "soundOn": false
            },
            "version": 3
        });
        let _ = fs::write(&launcher_profiles_path, dummy_profiles.to_string());
    }

    let server_target_dir = instance_path.join("server");
    let _ = fs::create_dir_all(&server_target_dir);

    // 4. Dispatch Loader Provisioning
    match chosen_loader.to_lowercase().as_str() {
        "fabric" => {
            install_fabric(&app, &client, &java_bin, &instance_path, &chosen_version).await?;
        }
        "quilt" => {
            install_quilt(&app, &client, &java_bin, &instance_path, &chosen_version).await?;
        }
        "paper" => {
            install_paper(&app, &client, &server_target_dir, &chosen_version).await?;
        }
        "purpur" => {
            install_purpur(&app, &client, &server_target_dir, &chosen_version).await?;
        }
        "vanilla" => {
            let _ = app.emit(
                "install-progress",
                InstallProgress {
                    step: "Vanilla Provisioning".into(),
                    percentage: 90.0,
                    details: format!("Vanilla Minecraft {chosen_version} runtime ready!"),
                },
            );
        }
        _ => {
            // Default to NeoForge
            install_neoforge(&app, &client, &java_bin, &instance_path, &chosen_version).await?;
        }
    }

    // Maintain zero data duplication by junctioning server libraries & config to .minecraft
    let _ = crate::mods::ensure_zero_duplication_junctions(&instance_path);

    // 5. Write eula.txt
    let eula_file = server_target_dir.join("eula.txt");
    let _ = fs::write(eula_file, "eula=true\n");

    // 6. Write server.properties default
    let props_file = server_target_dir.join("server.properties");
    if !props_file.exists() {
        let properties_content = "\
level-name=../.minecraft/saves/World1\n\
online-mode=true\n\
simulation-distance=6\n\
view-distance=10\n\
sync-chunk-writes=false\n\
server-port=25565\n\
enable-rcon=false\n";
        let _ = fs::write(props_file, properties_content);
    }

    let _ = app.emit(
        "install-progress",
        InstallProgress {
            step: "Complete".into(),
            percentage: 100.0,
            details: format!("{chosen_loader} {chosen_version} instance installed and ready!"),
        },
    );

    let _ = state;
    Ok(())
}
