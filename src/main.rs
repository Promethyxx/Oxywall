use anyhow::Result;
use dotenvy::dotenv;
use rand::prelude::IndexedRandom; // <-- CORRECTION: Nouveau trait pour rand 0.10
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashSet;
use std::env;
use std::path::Path;
use tokio::fs::{create_dir_all, File, OpenOptions}; // <-- CORRECTION: OpenOptions pour l'append
use tokio::io::AsyncWriteExt;
use tokio::time::{sleep, Duration};

pub const LOGO_DARK: &[u8] = include_bytes!("../assets/Oxywall_dark.png");
pub const LOGO_LIGHT: &[u8] = include_bytes!("../assets/Oxywall_light.png");
pub const LOGO_ICON_PNG: &[u8] = include_bytes!("../assets/Oxywall_icon.png");

// -----------------------------------------------------------------------------
// Constantes
// -----------------------------------------------------------------------------
const OUTPUT_DIR: &str = "wallpapers";
const LOG_FILE: &str = "wallpapers/downloaded.txt";
const MASTER_THEMES: usize = 3;
const SUB_PER_THEME: usize = 5;

// Dimensions valides : 3840x2160 ou 1920x1080
const VALID_SIZES: [(u32, u32); 2] = [(3840, 2160), (1920, 1080)];

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
// Gestion du log des IDs déjà téléchargés
// -----------------------------------------------------------------------------
async fn load_log() -> Result<HashSet<String>> {
    let path = Path::new(LOG_FILE);
    if !path.exists() {
        return Ok(HashSet::new());
    }
    let content = tokio::fs::read_to_string(path).await?;
    let ids: HashSet<String> = content.lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    Ok(ids)
}

async fn save_log(id: &str) -> Result<()> {
    let path = Path::new(LOG_FILE);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    file.write_all(format!("{}\n", id).as_bytes()).await?;
    Ok(())
}

// -----------------------------------------------------------------------------
// Fonctions de fetch par source
// -----------------------------------------------------------------------------
async fn fetch_pexels(client: &Client, query: &str, max_photos: usize, api_key: &str) -> Result<Vec<Image>> {
    println!("  [Pexels] '{}'", query);
    let mut found = Vec::new();
    let mut page = 1;

    // CORRECTION : Encodage basique des espaces pour l'URL
    let encoded_query = query.replace(' ', "%20");

    while found.len() < max_photos {
        // CORRECTION : Contournement du problème .query() en formatant l'URL directement
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
            println!("  [Pexels] Erreur HTTP {}", resp.status());
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

    println!("  [Pexels] {} images valides trouvées", found.len());
    Ok(found.into_iter().take(max_photos).collect())
}

async fn fetch_unsplash(client: &Client, query: &str, max_photos: usize, api_key: &str) -> Result<Vec<Image>> {
    println!("  [Unsplash] '{}'", query);
    let mut found = Vec::new();
    let mut page = 1;

    let encoded_query = query.replace(' ', "%20");

    while found.len() < max_photos {
        let url = format!(
            "https://api.unsplash.com/search/photos?query={}&per_page=30&page={}&orientation=landscape&client_id={}",
            encoded_query, page, api_key
        );

        let resp = client
            .get(&url)
            .send()
            .await?;

        if !resp.status().is_success() {
            println!("  [Unsplash] Erreur HTTP {}", resp.status());
            break;
        }

        let data: UnsplashSearch = resp.json().await?;
        if data.results.is_empty() {
            break;
        }

        for p in data.results {
            let img = Image {
                id: format!("unsplash_{}", p.id),
                url: format!("{}&w={}&h={}&fit=max&fm=jpg&q=100", p.urls.raw, p.width, p.height),
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

    println!("  [Unsplash] {} images valides trouvées", found.len());
    Ok(found.into_iter().take(max_photos).collect())
}

async fn fetch_pixabay(client: &Client, query: &str, max_photos: usize, api_key: &str) -> Result<Vec<Image>> {
    println!("  [Pixabay] '{}'", query);
    let mut found = Vec::new();
    let mut page = 1;

    let encoded_query = query.replace(' ', "%20");

    while found.len() < max_photos {
        let url = format!(
            "https://pixabay.com/api/?key={}&q={}&image_type=photo&orientation=horizontal&per_page=200&page={}&safesearch=true",
            api_key, encoded_query, page
        );

        let resp = client
            .get(&url)
            .send()
            .await?;

        if !resp.status().is_success() {
            println!("  [Pixabay] Erreur HTTP {}", resp.status());
            break;
        }

        let data: PixabaySearch = resp.json().await?;
        let hits_len = data.hits.len();
        if hits_len == 0 {
            break;
        }

        for p in data.hits {
            let url = p.largeImageURL.as_ref().or(p.webformatURL.as_ref()).map(|s| s.to_string());
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

    println!("  [Pixabay] {} images valides trouvées", found.len());
    Ok(found.into_iter().take(max_photos).collect())
}

// -----------------------------------------------------------------------------
// Téléchargement pour une requête donnée
// -----------------------------------------------------------------------------
async fn download_all(client: &Client, query: &str, max_per_source: usize, already: &HashSet<String>) -> Result<usize> {
    let folder = Path::new(OUTPUT_DIR).join(query.replace(' ', "_"));
    create_dir_all(&folder).await?;
    create_dir_all(OUTPUT_DIR).await?;

    println!("\n=======================================================");
    println!("  Recherche : {}", query);
    println!("  Déjà téléchargés (log) : {}", already.len());
    println!("=======================================================");

    let mut sources = Vec::new();

    let pexels_key = env::var("PEXELS_API_KEY").unwrap_or_default();
    if !pexels_key.is_empty() {
        sources.extend(fetch_pexels(client, query, max_per_source, &pexels_key).await?);
    }

    let unsplash_key = env::var("UNSPLASH_API_KEY").unwrap_or_default();
    if !unsplash_key.is_empty() {
        sources.extend(fetch_unsplash(client, query, max_per_source, &unsplash_key).await?);
    }

    let pixabay_key = env::var("PIXABAY_API_KEY").unwrap_or_default();
    if !pixabay_key.is_empty() {
        sources.extend(fetch_pixabay(client, query, max_per_source, &pixabay_key).await?);
    }

    // Dédoublonnage et filtre log
    let mut seen = HashSet::new();
    let unique: Vec<Image> = sources
        .into_iter()
        .filter(|img| !already.contains(&img.id) && seen.insert(img.id.clone()))
        .collect();

    println!("\n📦 {} nouvelles images à télécharger\n", unique.len());

    let mut downloaded = 0;
    for (_idx, img) in unique.iter().enumerate() {
        let filename = folder.join(format!("{}_{}.jpg", img.id, img.dim_str()));
        let response = client.get(&img.url).send().await?;
        if response.status().is_success() {
            let bytes = response.bytes().await?;
            let mut file = File::create(&filename).await?;
            file.write_all(&bytes).await?;
            save_log(&img.id).await?;
            downloaded += 1;
            println!("  ✅ [{}/{}] {} — {}", downloaded, unique.len(), img.dim_str(), img.id);
        } else {
            println!("  ⚠️  HTTP {} — {}", response.status(), img.id);
        }
        sleep(Duration::from_millis(100)).await;
    }

    println!("\n✅ {} nouveaux wallpapers dans '{}'", downloaded, folder.display());
    Ok(downloaded)
}

// -----------------------------------------------------------------------------
// Thèmes (via lazy_static)
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
// Main
// -----------------------------------------------------------------------------
#[tokio::main]
async fn main() -> Result<()> {
    // -------------------------------------------------------------------------
    // Résolution robuste du .env
    // -------------------------------------------------------------------------
    let current_cwd_env = env::current_dir().unwrap_or_default().join(".env");
    let exe_dir_env = env::current_exe().unwrap_or_default().parent().unwrap_or(Path::new("")).join(".env");

    if current_cwd_env.exists() {
        dotenvy::from_path(&current_cwd_env).ok();
    } else if exe_dir_env.exists() {
        dotenvy::from_path(&exe_dir_env).ok();
    } else {
        dotenv().ok(); // Fallback par défaut
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    println!("=======================================================");
    println!("  Wallpaper Downloader — Mode aléatoire (Rust)");
    println!("  3840x2160 / 1920x1080 strict");
    println!("=======================================================");

    let already = load_log().await?;

    // CORRECTION : nouvelle méthode de RNG pour rand 0.10
    let mut rng = rand::rng();
    let masters: Vec<&str> = THEMES
        .choose_multiple(&mut rng, MASTER_THEMES)
        .map(|(name, _)| *name)
        .collect();

    println!("\n🎲 Thèmes maîtres tirés : {}\n", masters.join(", "));

    let mut total_downloaded = 0;

    for master in masters {
        let subs = THEMES.iter().find(|(name, _)| *name == master).map(|(_, subs)| subs).unwrap();
        let picked: Vec<&str> = subs.choose_multiple(&mut rng, SUB_PER_THEME).copied().collect();

        println!("\n───────────────────────────────────────────────────────");
        println!("  🎨 {}", master.to_uppercase());
        println!("  Déclinaisons : {}", picked.join(", "));
        println!("───────────────────────────────────────────────────────");

        for sub in picked {
            let n = download_all(&client, sub, 50, &already).await?;
            total_downloaded += n;
        }
    }

    println!("\n=======================================================");
    println!("🎉 Terminé ! {} nouveaux wallpapers au total", total_downloaded);
    println!("📁 Dossier : ./{}/", OUTPUT_DIR);
    println!("📋 Log : ./{}", LOG_FILE);
    println!("=======================================================");

    Ok(())
}