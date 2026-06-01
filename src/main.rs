use anyhow::Result;
use dotenvy::dotenv;
use eframe::egui;
use rand::prelude::IndexedRandom;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use tokio::fs::{create_dir_all, File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::time::{sleep, Duration};

// -----------------------------------------------------------------------------
// Assets embarqués
// -----------------------------------------------------------------------------
pub const LOGO_DARK: &[u8] = include_bytes!("../assets/Oxywall_dark.png");
pub const LOGO_LIGHT: &[u8] = include_bytes!("../assets/Oxywall_light.png");
pub const LOGO_ICON_PNG: &[u8] = include_bytes!("../assets/Oxywall_icon.png");

// -----------------------------------------------------------------------------
// Constantes
// -----------------------------------------------------------------------------
const MASTER_THEMES: usize = 3;
const SUB_PER_THEME: usize = 5;
const VALID_SIZES: [(u32, u32); 2] = [(3840, 2160), (1920, 1080)];
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

// -----------------------------------------------------------------------------
// Structures pour les réponses API
// -----------------------------------------------------------------------------
#[derive(Debug, Deserialize)]
struct PexelsSearch {
    photos: Vec<PexelsPhoto>,
    next_page: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PexelsPhoto {
    id: u32,
    width: u32,
    height: u32,
    src: PexelsSrc,
}

#[derive(Debug, Deserialize)]
struct PexelsSrc {
    original: String,
}

#[derive(Debug, Deserialize)]
struct UnsplashSearch {
    results: Vec<UnsplashPhoto>,
}

#[derive(Debug, Deserialize)]
struct UnsplashPhoto {
    id: String,
    width: u32,
    height: u32,
    urls: UnsplashUrls,
}

#[derive(Debug, Deserialize)]
struct UnsplashUrls {
    raw: String,
}

#[derive(Debug, Deserialize)]
struct PixabaySearch {
    hits: Vec<PixabayHit>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct PixabayHit {
    id: u32,
    imageWidth: u32,
    imageHeight: u32,
    largeImageURL: Option<String>,
    webformatURL: Option<String>,
}

// -----------------------------------------------------------------------------
// Image unifiée
// -----------------------------------------------------------------------------
#[derive(Debug, Clone)]
struct Image {
    id: String,
    url: String,
    width: u32,
    height: u32,
}

impl Image {
    fn dim_str(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }

    fn is_valid_size(&self) -> bool {
        VALID_SIZES.contains(&(self.width, self.height))
    }
}

// -----------------------------------------------------------------------------
// Log des IDs déjà téléchargés
// -----------------------------------------------------------------------------
async fn load_log(log_file: &Path) -> Result<HashSet<String>> {
    if !log_file.exists() {
        return Ok(HashSet::new());
    }
    let content = tokio::fs::read_to_string(log_file).await?;
    let ids: HashSet<String> = content
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    Ok(ids)
}

async fn save_log(log_file: &Path, id: &str) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)
        .await?;
    file.write_all(format!("{}\n", id).as_bytes()).await?;
    Ok(())
}

// -----------------------------------------------------------------------------
// Fetch Pexels
// -----------------------------------------------------------------------------
async fn fetch_pexels(
    client: &Client,
    query: &str,
    max_photos: usize,
    api_key: &str,
) -> Result<Vec<Image>> {
    let mut found = Vec::new();
    let mut page = 1;
    let encoded_query = query.replace(' ', "%20");

    while found.len() < max_photos {
        let url = format!(
            "https://api.pexels.com/v1/search?query={}&per_page=80&page={}&orientation=landscape",
            encoded_query, page
        );
        let resp = client
            .get(&url)
            .header("Authorization", api_key)
            .send()
            .await?;
        if !resp.status().is_success() {
            break;
        }
        let data: PexelsSearch = resp.json().await?;
        if data.photos.is_empty() {
            break;
        }
        for p in data.photos {
            let img = Image {
                id: format!("pexels_{}", p.id),
                url: p.src.original,
                width: p.width,
                height: p.height,
            };
            if img.is_valid_size() {
                found.push(img);
            }
        }
        if data.next_page.is_none() {
            break;
        }
        page += 1;
        sleep(Duration::from_millis(200)).await;
    }
    Ok(found.into_iter().take(max_photos).collect())
}

// -----------------------------------------------------------------------------
// Fetch Unsplash
// -----------------------------------------------------------------------------
async fn fetch_unsplash(
    client: &Client,
    query: &str,
    max_photos: usize,
    api_key: &str,
) -> Result<Vec<Image>> {
    let mut found = Vec::new();
    let mut page = 1;
    let encoded_query = query.replace(' ', "%20");

    while found.len() < max_photos {
        let url = format!(
            "https://api.unsplash.com/search/photos?query={}&per_page=30&page={}&orientation=landscape&client_id={}",
            encoded_query, page, api_key
        );
        let resp = client.get(&url).send().await?;
        if !resp.status().is_success() {
            break;
        }
        let data: UnsplashSearch = resp.json().await?;
        if data.results.is_empty() {
            break;
        }
        for p in data.results {
            let img = Image {
                id: format!("unsplash_{}", p.id),
                url: format!(
                    "{}&w={}&h={}&fit=max&fm=jpg&q=100",
                    p.urls.raw, p.width, p.height
                ),
                width: p.width,
                height: p.height,
            };
            if img.is_valid_size() {
                found.push(img);
            }
        }
        page += 1;
        sleep(Duration::from_millis(200)).await;
    }
    Ok(found.into_iter().take(max_photos).collect())
}

// -----------------------------------------------------------------------------
// Fetch Pixabay
// -----------------------------------------------------------------------------
async fn fetch_pixabay(
    client: &Client,
    query: &str,
    max_photos: usize,
    api_key: &str,
) -> Result<Vec<Image>> {
    let mut found = Vec::new();
    let mut page = 1;
    let encoded_query = query.replace(' ', "%20");

    while found.len() < max_photos {
        let url = format!(
            "https://pixabay.com/api/?key={}&q={}&image_type=photo&orientation=horizontal&per_page=200&page={}&safesearch=true",
            api_key, encoded_query, page
        );
        let resp = client.get(&url).send().await?;
        if !resp.status().is_success() {
            break;
        }
        let data: PixabaySearch = resp.json().await?;
        let hits_len = data.hits.len();
        if hits_len == 0 {
            break;
        }
        for p in data.hits {
            let url = p
                .largeImageURL
                .as_ref()
                .or(p.webformatURL.as_ref())
                .map(|s| s.to_string());
            if let Some(url) = url {
                let img = Image {
                    id: format!("pixabay_{}", p.id),
                    url,
                    width: p.imageWidth,
                    height: p.imageHeight,
                };
                if img.is_valid_size() {
                    found.push(img);
                }
            }
        }
        if hits_len < 200 {
            break;
        }
        page += 1;
        sleep(Duration::from_millis(200)).await;
    }
    Ok(found.into_iter().take(max_photos).collect())
}

// -----------------------------------------------------------------------------
// Téléchargement pour une requête donnée
// -----------------------------------------------------------------------------
async fn download_all(
    client: &Client,
    query: &str,
    max_per_source: usize,
    already: &HashSet<String>,
    log_tx: &mpsc::Sender<String>,
    output_dir: &Path,
    log_file: &Path,
) -> Result<usize> {
    let folder = output_dir.join(query.replace(' ', "_"));
    create_dir_all(&folder).await?;
    create_dir_all(output_dir).await?;

    let _ = log_tx.send(format!("Searching: {}", query));

    let mut sources = Vec::new();

    let pexels_key = env::var("PEXELS_API_KEY").unwrap_or_default();
    if !pexels_key.is_empty() {
        let _ = log_tx.send(format!("  [Pexels] '{}'", query));
        sources.extend(fetch_pexels(client, query, max_per_source, &pexels_key).await?);
        let _ = log_tx.send(format!("  [Pexels] {} valid images", sources.len()));
    }

    let unsplash_key = env::var("UNSPLASH_API_KEY").unwrap_or_default();
    if !unsplash_key.is_empty() {
        let before = sources.len();
        let _ = log_tx.send(format!("  [Unsplash] '{}'", query));
        sources.extend(fetch_unsplash(client, query, max_per_source, &unsplash_key).await?);
        let _ = log_tx.send(format!("  [Unsplash] {} valid images", sources.len() - before));
    }

    let pixabay_key = env::var("PIXABAY_API_KEY").unwrap_or_default();
    if !pixabay_key.is_empty() {
        let before = sources.len();
        let _ = log_tx.send(format!("  [Pixabay] '{}'", query));
        sources.extend(fetch_pixabay(client, query, max_per_source, &pixabay_key).await?);
        let _ = log_tx.send(format!("  [Pixabay] {} valid images", sources.len() - before));
    }

    let mut seen = HashSet::new();
    let unique: Vec<Image> = sources
        .into_iter()
        .filter(|img| !already.contains(&img.id) && seen.insert(img.id.clone()))
        .collect();

    let _ = log_tx.send(format!("{} new images to download", unique.len()));

    let mut downloaded = 0;
    for img in &unique {
        let filename = folder.join(format!("{}_{}.jpg", img.id, img.dim_str()));
        let response = client.get(&img.url).send().await?;
        if response.status().is_success() {
            let bytes = response.bytes().await?;
            let mut file = File::create(&filename).await?;
            file.write_all(&bytes).await?;
            save_log(log_file, &img.id).await?;
            downloaded += 1;
            let _ = log_tx.send(format!(
                "  ✅ [{}/{}] {} — {}",
                downloaded,
                unique.len(),
                img.dim_str(),
                img.id
            ));
        } else {
            let _ = log_tx.send(format!("  ⚠️  HTTP {} — {}", response.status(), img.id));
        }
        sleep(Duration::from_millis(100)).await;
    }

    let _ = log_tx.send(format!(
        "✅ {} new wallpapers in '{}'",
        downloaded,
        folder.display()
    ));
    Ok(downloaded)
}

// -----------------------------------------------------------------------------
// Thèmes
// -----------------------------------------------------------------------------
lazy_static::lazy_static! {
    static ref THEMES: Vec<(&'static str, Vec<&'static str>)> = vec![
        ("nature landscape", vec![
            "nature landscape", "countryside fields", "valley panorama",
            "river stream nature", "waterfall tropical", "prairie wildflowers",
            "savanna landscape", "desert dunes", "canyon landscape",
            "lake reflection nature", "spring blossoms landscape", "autumn foliage scenery",
        ]),
        ("space galaxy", vec![
            "space galaxy", "nebula stars", "milky way night sky",
            "planet surface", "solar system", "aurora borealis sky",
            "cosmos deep space", "starfield universe", "supernova explosion",
            "astronaut space", "space station orbit", "moon surface craters",
        ]),
        ("architecture", vec![
            "architecture building", "modern architecture", "gothic cathedral",
            "skyscraper cityscape", "ancient ruins", "japanese temple",
            "brutalist architecture", "art deco building", "bridge engineering",
            "interior design luxury", "mosque architecture", "castle medieval",
        ]),
        ("abstract", vec![
            "abstract colorful", "geometric patterns", "fractal art",
            "abstract gradient", "minimalist abstract", "liquid abstract",
            "abstract smoke", "neon abstract", "abstract texture",
            "abstract waves", "marble texture abstract", "abstract light trails",
        ]),
        ("mountains", vec![
            "mountains landscape", "snowy mountain peaks", "mountain lake alpine",
            "himalaya mountains", "volcano landscape", "mountain forest fog",
            "rocky mountains", "mountain sunset", "dolomites italy",
            "mountain trail hiking", "mountain clouds aerial", "fjord mountains norway",
        ]),
        ("art", vec![
            "renaissance painting", "oil painting classic", "impressionist art",
            "baroque art painting", "watercolor painting", "surrealist art",
            "art nouveau illustration", "classical sculpture", "fresco painting",
            "romantic era painting", "ukiyo-e japanese art", "mosaic art ancient",
        ]),
        ("animaux", vec![
            "wildlife animal", "lion portrait", "eagle bird flight",
            "underwater fish coral", "wolf forest", "elephant savanna",
            "butterfly macro", "horse running", "owl night bird",
            "tiger jungle", "bear wilderness", "fox nature wildlife",
        ]),
        ("ocean", vec![
            "ocean waves", "underwater coral reef", "deep sea creatures",
            "tropical beach turquoise", "ocean aerial view", "whale underwater",
            "surfing big wave", "jellyfish underwater", "shipwreck diving",
            "ocean sunset horizon", "manta ray underwater", "arctic ice ocean",
        ]),
        ("city", vec![
            "city skyline night", "neon city street", "tokyo night city",
            "new york skyline", "cyberpunk city", "rainy city street",
            "city aerial view", "hong kong cityscape", "city traffic lights",
            "dubai skyline", "paris city night", "urban street photography",
        ]),
        ("forest", vec![
            "dark forest fog", "enchanted forest", "bamboo forest",
            "autumn forest path", "tropical rainforest", "forest sunlight rays",
            "redwood giant trees", "forest snow winter", "mossy forest creek",
            "birch forest white", "forest aerial drone", "jungle dense vegetation",
        ]),
        ("macro", vec![
            "macro water drops", "macro insect eyes", "macro flower petals",
            "snowflake macro crystal", "macro spider web dew", "macro feather detail",
            "macro leaf veins", "macro bubbles", "macro rust texture",
            "macro ice crystals", "macro dandelion seeds", "macro gemstone mineral",
        ]),
        ("weather", vec![
            "lightning storm", "aurora borealis", "tornado storm",
            "dramatic clouds sky", "rainbow landscape", "blizzard snow storm",
            "fog mist morning", "sandstorm desert", "sunset dramatic sky",
            "thunder dark clouds", "ice storm frozen", "monsoon rain tropical",
        ]),
        ("vehicles", vec![
            "supercar sports car", "motorcycle road", "vintage classic car",
            "fighter jet aircraft", "sailboat ocean", "train landscape scenic",
            "helicopter aerial", "racing car track", "spaceship concept",
            "submarine underwater", "hot air balloon", "off road 4x4 adventure",
        ]),
        ("fantasy", vec![
            "fantasy landscape castle", "dragon fantasy art", "fantasy forest magical",
            "concept art sci-fi", "fantasy underwater city", "steampunk illustration",
            "dark fantasy artwork", "celestial fantasy art", "mythical creatures art",
            "enchanted kingdom", "fantasy warrior artwork", "alien planet landscape",
        ]),
        ("pop culture", vec![
            "anime wallpaper", "anime scenery", "manga art style",
            "video game screenshot", "retro gaming pixel art", "pop art colorful",
            "synthwave retrowave", "vaporwave aesthetic", "comic book art",
            "cyberpunk anime", "studio ghibli style", "neon retro 80s",
        ]),
    ];
}

// -----------------------------------------------------------------------------
// Messages de contrôle vers le thread download
// -----------------------------------------------------------------------------
enum DownloadMsg {
    Log(String),
    // Signal que les clés ont changé (chargement .env en cours de run — non utilisé ici
    // mais prévu pour extension future)
}

// -----------------------------------------------------------------------------
// Core download logic
// -----------------------------------------------------------------------------
fn run_download(
    log_tx: mpsc::Sender<String>,
    output_dir: PathBuf,
    log_file: PathBuf,
) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    rt.block_on(async move {
        let client = match Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = log_tx.send(format!("❌ HTTP client error: {}", e));
                return;
            }
        };

        let already = match load_log(&log_file).await {
            Ok(a) => a,
            Err(e) => {
                let _ = log_tx.send(format!("❌ Log read error: {}", e));
                return;
            }
        };

        let mut rng = rand::rng();
        let masters: Vec<&str> = THEMES
            .sample(&mut rng, MASTER_THEMES)
            .map(|(name, _)| *name)
            .collect();

        let _ = log_tx.send(format!("🎲 Selected themes: {}", masters.join(", ")));

        let mut total = 0usize;

        for master in &masters {
            let subs = THEMES
                .iter()
                .find(|(name, _)| name == master)
                .map(|(_, subs)| subs)
                .unwrap();
            let picked: Vec<&str> = subs
                .sample(&mut rng, SUB_PER_THEME)
                .copied()
                .collect();

            let _ = log_tx.send(format!(
                "─── 🎨 {} — {}",
                master.to_uppercase(),
                picked.join(", ")
            ));

            for sub in picked {
                match download_all(
                    &client,
                    sub,
                    50,
                    &already,
                    &log_tx,
                    &output_dir,
                    &log_file,
                )
                .await
                {
                    Ok(n) => total += n,
                    Err(e) => {
                        let _ = log_tx.send(format!("❌ Error on '{}': {}", sub, e));
                    }
                }
            }
        }

        let _ = log_tx.send(format!(
            "🎉 Done! {} new wallpapers total — {}",
            total,
            output_dir.display()
        ));
    });
}

// -----------------------------------------------------------------------------
// GUI — helper: charger un PNG embarqué en TextureHandle
// -----------------------------------------------------------------------------
fn load_texture(ctx: &egui::Context, name: &str, bytes: &[u8]) -> egui::TextureHandle {
    let image = image::load_from_memory(bytes)
        .expect("Failed to decode image")
        .to_rgba8();
    let (w, h) = image.dimensions();
    let color_image = egui::ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize],
        image.as_raw(),
    );
    ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR)
}

// -----------------------------------------------------------------------------
// GUI — état du dialog en cours (non-bloquant)
// -----------------------------------------------------------------------------
enum DialogPending {
    None,
    EnvFile(std::sync::Arc<std::sync::Mutex<Option<Option<PathBuf>>>>),
    OutputDir(std::sync::Arc<std::sync::Mutex<Option<Option<PathBuf>>>>),
}

// -----------------------------------------------------------------------------
// GUI — App state
// -----------------------------------------------------------------------------
struct OxywallApp {
    // Theme
    dark_mode: bool,
    tex_dark: egui::TextureHandle,
    tex_light: egui::TextureHandle,

    // API keys
    pexels_key: String,
    unsplash_key: String,
    pixabay_key: String,

    // Paths
    env_path: String,       // chemin du .env affiché
    output_dir: String,     // dossier de destination

    // Dialog state
    dialog_pending: DialogPending,

    // Download state
    is_running: bool,
    log_lines: Vec<String>,
    log_rx: Option<mpsc::Receiver<String>>,
}

impl OxywallApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = &cc.egui_ctx;
        ctx.set_visuals(egui::Visuals::dark());

        let tex_dark = load_texture(ctx, "logo_dark", LOGO_DARK);
        let tex_light = load_texture(ctx, "logo_light", LOGO_LIGHT);

        // Dossier par défaut : à côté de l'exe
        let default_output = env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("wallpapers")))
            .unwrap_or_else(|| PathBuf::from("wallpapers"));

        // Pré-remplir les clés depuis l'env si déjà définies
        let pexels_key = env::var("PEXELS_API_KEY").unwrap_or_default();
        let unsplash_key = env::var("UNSPLASH_API_KEY").unwrap_or_default();
        let pixabay_key = env::var("PIXABAY_API_KEY").unwrap_or_default();

        Self {
            dark_mode: true,
            tex_dark,
            tex_light,
            pexels_key,
            unsplash_key,
            pixabay_key,
            env_path: String::new(),
            output_dir: default_output.to_string_lossy().into_owned(),
            dialog_pending: DialogPending::None,
            is_running: false,
            log_lines: Vec::new(),
            log_rx: None,
        }
    }

    /// Charge un fichier .env et met à jour les champs de clés.
    fn load_env_file(&mut self, path: &Path) {
        self.env_path = path.to_string_lossy().into_owned();
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines() {
                let line = line.trim();
                if line.starts_with('#') || line.is_empty() {
                    continue;
                }
                if let Some((key, val)) = line.split_once('=') {
                    let val = val.trim_matches('"').trim_matches('\'').trim();
                    match key.trim() {
                        "PEXELS_API_KEY" => self.pexels_key = val.to_string(),
                        "UNSPLASH_API_KEY" => self.unsplash_key = val.to_string(),
                        "PIXABAY_API_KEY" => self.pixabay_key = val.to_string(),
                        _ => {}
                    }
                }
            }
        }
    }

    fn start_download(&mut self, ctx: &egui::Context) {
        // Injecter les clés dans l'env pour les fonctions async
        unsafe {
            env::set_var("PEXELS_API_KEY", &self.pexels_key);
            env::set_var("UNSPLASH_API_KEY", &self.unsplash_key);
            env::set_var("PIXABAY_API_KEY", &self.pixabay_key);
        }

        let output_dir = PathBuf::from(&self.output_dir);
        let log_file = output_dir.join("downloaded.txt");

        let (tx, rx) = mpsc::channel::<String>();
        self.log_rx = Some(rx);
        self.is_running = true;
        self.log_lines.clear();
        self.log_lines.push(format!(
            "Output dir: {}",
            output_dir.display()
        ));

        let ctx_clone = ctx.clone();
        std::thread::spawn(move || {
            run_download(tx, output_dir, log_file);
            ctx_clone.request_repaint();
        });
    }

    /// Lance un dialog natif dans un thread séparé et stocke le résultat
    /// dans un Arc<Mutex<Option<Option<PathBuf>>>> partagé.
    ///  - None         = dialog encore ouvert
    ///  - Some(None)   = annulé
    ///  - Some(Some(p))= chemin choisi
    fn open_env_dialog(&mut self, ctx: &egui::Context) {
        let result: std::sync::Arc<std::sync::Mutex<Option<Option<PathBuf>>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let result_clone = result.clone();
        let ctx_clone = ctx.clone();
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .add_filter("Env file", &["env", "txt"])
                .set_title("Open .env file")
                .pick_file();
            *result_clone.lock().unwrap() = Some(picked);
            ctx_clone.request_repaint();
        });
        self.dialog_pending = DialogPending::EnvFile(result);
    }

    fn open_output_dir_dialog(&mut self, ctx: &egui::Context) {
        let result: std::sync::Arc<std::sync::Mutex<Option<Option<PathBuf>>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let result_clone = result.clone();
        let ctx_clone = ctx.clone();
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("Choose wallpaper output folder")
                .pick_folder();
            *result_clone.lock().unwrap() = Some(picked);
            ctx_clone.request_repaint();
        });
        self.dialog_pending = DialogPending::OutputDir(result);
    }

    /// Appeler chaque frame pour récupérer le résultat d'un dialog en cours.
    fn poll_dialog(&mut self) {
        let resolved = match &self.dialog_pending {
            DialogPending::None => return,
            DialogPending::EnvFile(arc) => {
                let guard = arc.lock().unwrap();
                guard.clone().map(|r| (true, r))
            }
            DialogPending::OutputDir(arc) => {
                let guard = arc.lock().unwrap();
                guard.clone().map(|r| (false, r))
            }
        };

        if let Some((is_env, maybe_path)) = resolved {
            if is_env {
                if let Some(path) = maybe_path {
                    self.load_env_file(&path);
                }
            } else {
                if let Some(path) = maybe_path {
                    self.output_dir = path.to_string_lossy().into_owned();
                }
            }
            self.dialog_pending = DialogPending::None;
        }
    }
}

impl eframe::App for OxywallApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Récupérer le résultat des dialogs non-bloquants
        self.poll_dialog();

        // Drainer le channel de logs
        if let Some(rx) = &self.log_rx {
            while let Ok(msg) = rx.try_recv() {
                if msg.starts_with("🎉") {
                    self.is_running = false;
                }
                self.log_lines.push(msg);
            }
        }
        if self.is_running {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }

        // ── Header ──────────────────────────────────────────────────────────
        egui::Panel::top("header")
            .min_size(72.0)
            .show_inside(ui, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let tex = if self.dark_mode { &self.tex_dark } else { &self.tex_light };
                    let logo_h = 52.0;
                    let aspect = tex.size_vec2().x / tex.size_vec2().y;
                    ui.add(
                        egui::Image::new(tex)
                            .fit_to_exact_size(egui::vec2(logo_h * aspect, logo_h)),
                    );
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(format!("Oxywall {}", APP_VERSION))
                                .size(22.0)
                                .strong(),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let icon = if self.dark_mode { "☀ Light" } else { "🌙 Dark" };
                        if ui.button(icon).clicked() {
                            self.dark_mode = !self.dark_mode;
                            ctx.set_visuals(if self.dark_mode {
                                egui::Visuals::dark()
                            } else {
                                egui::Visuals::light()
                            });
                        }
                    });
                });
                ui.add_space(6.0);
            });

        // ── Corps ────────────────────────────────────────────────────────────
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.add_space(8.0);

            // ── Section : fichier .env ────────────────────────────────────
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(egui::RichText::new("Environment file (.env)").strong());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.env_path)
                            .hint_text("Path to .env file (optional)")
                            .desired_width(ui.available_width() - 70.0),
                    );
                    let browsing_env = matches!(self.dialog_pending, DialogPending::EnvFile(_));
                    if ui.add_enabled(!browsing_env, egui::Button::new("Browse…")).clicked() {
                        self.open_env_dialog(&ctx);
                    }
                });
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new("Opening a .env will auto-fill the API keys below.")
                        .small()
                        .weak(),
                );
            });

            ui.add_space(8.0);

            // ── Section : API Keys ────────────────────────────────────────
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(egui::RichText::new("API Keys").strong());
                ui.add_space(4.0);

                egui::Grid::new("api_keys_grid")
                    .num_columns(3)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        // Pexels
                        ui.label("Pexels");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.pexels_key)
                                .hint_text("PEXELS_API_KEY")
                                .password(true)
                                .desired_width(280.0),
                        );
                        if ui.button("📋 Copy").clicked() {
                            ctx.copy_text(self.pexels_key.clone());
                        }
                        ui.end_row();

                        // Unsplash
                        ui.label("Unsplash");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.unsplash_key)
                                .hint_text("UNSPLASH_ACCESS_KEY")
                                .password(true)
                                .desired_width(280.0),
                        );
                        if ui.button("📋 Copy").clicked() {
                            ctx.copy_text(self.unsplash_key.clone());
                        }
                        ui.end_row();

                        // Pixabay
                        ui.label("Pixabay");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.pixabay_key)
                                .hint_text("PIXABAY_API_KEY")
                                .password(true)
                                .desired_width(280.0),
                        );
                        if ui.button("📋 Copy").clicked() {
                            ctx.copy_text(self.pixabay_key.clone());
                        }
                        ui.end_row();
                    });
            });

            ui.add_space(8.0);

            // ── Section : dossier de sortie ───────────────────────────────
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(egui::RichText::new("Output folder").strong());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.output_dir)
                            .hint_text("Wallpaper output directory")
                            .desired_width(ui.available_width() - 70.0),
                    );
                    let browsing_dir = matches!(self.dialog_pending, DialogPending::OutputDir(_));
                    if ui.add_enabled(!browsing_dir, egui::Button::new("Browse…")).clicked() {
                        self.open_output_dir_dialog(&ctx);
                    }
                });
            });

            ui.add_space(10.0);

            // ── Bouton Get ────────────────────────────────────────────────
            ui.horizontal(|ui| {
                let btn_label = if self.is_running { "⏳ Running…" } else { "⬇ Get" };
                let btn = egui::Button::new(egui::RichText::new(btn_label).size(16.0));
                if ui.add_enabled(!self.is_running, btn).clicked() {
                    self.start_download(&ctx);
                }
                if self.is_running {
                    ui.spinner();
                }
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            // ── Log ───────────────────────────────────────────────────────
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &self.log_lines {
                        ui.label(egui::RichText::new(line).monospace().size(12.0));
                    }
                });
        });
    }
}

// -----------------------------------------------------------------------------
// Main
// -----------------------------------------------------------------------------
fn main() -> eframe::Result {
    let icon = {
        let img = image::load_from_memory(LOGO_ICON_PNG)
            .expect("icon decode")
            .to_rgba8();
        let (w, h) = img.dimensions();
        egui::IconData {
            rgba: img.into_raw(),
            width: w,
            height: h,
        }
    };

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("Oxywall {}", APP_VERSION))
            .with_inner_size([720.0, 600.0])
            .with_min_inner_size([500.0, 420.0])
            .with_icon(icon),
        ..Default::default()
    };

    eframe::run_native(
        "Oxywall",
        native_options,
        Box::new(|cc| Ok(Box::new(OxywallApp::new(cc)))),
    )
}