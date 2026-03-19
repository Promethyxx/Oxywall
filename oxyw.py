#!/usr/bin/env python3
"""
Wallpaper Downloader — Pexels + Unsplash + Pixabay
Filtre strict : uniquement 3840x2160 ou 1920x1080 natif
Log des IDs téléchargés → pas de doublons même si fichiers déplacés
Mode auto : pioche 3 thèmes maîtres au hasard, 5 déclinaisons chacun
"""

import random
import requests
import time
from pathlib import Path
from dotenv import load_dotenv
import os

# ============================================================
#  CLÉS API — à renseigner dans un fichier .env à côté du script
#  (voir .env.example)
# ============================================================
load_dotenv()
PEXELS_API_KEY   = os.getenv("PEXELS_API_KEY", "")
UNSPLASH_API_KEY = os.getenv("UNSPLASH_API_KEY", "")
PIXABAY_API_KEY  = os.getenv("PIXABAY_API_KEY", "")

OUTPUT_DIR  = "wallpapers"
VALID_SIZES = {(3840, 2160), (1920, 1080)}
LOG_FILE    = Path(OUTPUT_DIR) / "downloaded.txt"

MASTER_THEMES = 3       # Nombre de thèmes maîtres par run
SUB_PER_THEME = 5       # Nombre de déclinaisons par thème maître

# ─────────────────────────────────────────────
#  DICTIONNAIRE DE THÈMES
# ─────────────────────────────────────────────
THEMES = {
    "nature landscape": [
        "nature landscape", "countryside fields", "valley panorama",
        "river stream nature", "waterfall tropical", "prairie wildflowers",
        "savanna landscape", "desert dunes", "canyon landscape",
        "lake reflection nature", "spring blossoms landscape", "autumn foliage scenery",
    ],
    "space galaxy": [
        "space galaxy", "nebula stars", "milky way night sky",
        "planet surface", "solar system", "aurora borealis sky",
        "cosmos deep space", "starfield universe", "supernova explosion",
        "astronaut space", "space station orbit", "moon surface craters",
    ],
    "architecture": [
        "architecture building", "modern architecture", "gothic cathedral",
        "skyscraper cityscape", "ancient ruins", "japanese temple",
        "brutalist architecture", "art deco building", "bridge engineering",
        "interior design luxury", "mosque architecture", "castle medieval",
    ],
    "abstract": [
        "abstract colorful", "geometric patterns", "fractal art",
        "abstract gradient", "minimalist abstract", "liquid abstract",
        "abstract smoke", "neon abstract", "abstract texture",
        "abstract waves", "marble texture abstract", "abstract light trails",
    ],
    "mountains": [
        "mountains landscape", "snowy mountain peaks", "mountain lake alpine",
        "himalaya mountains", "volcano landscape", "mountain forest fog",
        "rocky mountains", "mountain sunset", "dolomites italy",
        "mountain trail hiking", "mountain clouds aerial", "fjord mountains norway",
    ],
    "art": [
        "renaissance painting", "oil painting classic", "impressionist art",
        "baroque art painting", "watercolor painting", "surrealist art",
        "art nouveau illustration", "classical sculpture", "fresco painting",
        "romantic era painting", "ukiyo-e japanese art", "mosaic art ancient",
    ],
    "animaux": [
        "wildlife animal", "lion portrait", "eagle bird flight",
        "underwater fish coral", "wolf forest", "elephant savanna",
        "butterfly macro", "horse running", "owl night bird",
        "tiger jungle", "bear wilderness", "fox nature wildlife",
    ],
    "ocean": [
        "ocean waves", "underwater coral reef", "deep sea creatures",
        "tropical beach turquoise", "ocean aerial view", "whale underwater",
        "surfing big wave", "jellyfish underwater", "shipwreck diving",
        "ocean sunset horizon", "manta ray underwater", "arctic ice ocean",
    ],
    "city": [
        "city skyline night", "neon city street", "tokyo night city",
        "new york skyline", "cyberpunk city", "rainy city street",
        "city aerial view", "hong kong cityscape", "city traffic lights",
        "dubai skyline", "paris city night", "urban street photography",
    ],
    "forest": [
        "dark forest fog", "enchanted forest", "bamboo forest",
        "autumn forest path", "tropical rainforest", "forest sunlight rays",
        "redwood giant trees", "forest snow winter", "mossy forest creek",
        "birch forest white", "forest aerial drone", "jungle dense vegetation",
    ],
    "macro": [
        "macro water drops", "macro insect eyes", "macro flower petals",
        "snowflake macro crystal", "macro spider web dew", "macro feather detail",
        "macro leaf veins", "macro bubbles", "macro rust texture",
        "macro ice crystals", "macro dandelion seeds", "macro gemstone mineral",
    ],
    "weather": [
        "lightning storm", "aurora borealis", "tornado storm",
        "dramatic clouds sky", "rainbow landscape", "blizzard snow storm",
        "fog mist morning", "sandstorm desert", "sunset dramatic sky",
        "thunder dark clouds", "ice storm frozen", "monsoon rain tropical",
    ],
    "vehicles": [
        "supercar sports car", "motorcycle road", "vintage classic car",
        "fighter jet aircraft", "sailboat ocean", "train landscape scenic",
        "helicopter aerial", "racing car track", "spaceship concept",
        "submarine underwater", "hot air balloon", "off road 4x4 adventure",
    ],
    "fantasy": [
        "fantasy landscape castle", "dragon fantasy art", "fantasy forest magical",
        "concept art sci-fi", "fantasy underwater city", "steampunk illustration",
        "dark fantasy artwork", "celestial fantasy art", "mythical creatures art",
        "enchanted kingdom", "fantasy warrior artwork", "alien planet landscape",
    ],
    "pop culture": [
        "anime wallpaper", "anime scenery", "manga art style",
        "video game screenshot", "retro gaming pixel art", "pop art colorful",
        "synthwave retrowave", "vaporwave aesthetic", "comic book art",
        "cyberpunk anime", "studio ghibli style", "neon retro 80s",
    ],
}

# ─────────────────────────────────────────────
#  LOG
# ─────────────────────────────────────────────
def load_log():
    """Charge les IDs déjà téléchargés."""
    if not LOG_FILE.exists():
        return set()
    with open(LOG_FILE, "r") as f:
        return set(line.strip() for line in f if line.strip())

def save_log(photo_id):
    """Ajoute un ID au log."""
    with open(LOG_FILE, "a") as f:
        f.write(photo_id + "\n")

# ─────────────────────────────────────────────
#  PEXELS
# ─────────────────────────────────────────────
def fetch_pexels(query, max_photos):
    print(f"\n  [Pexels] '{query}'")
    headers = {"Authorization": PEXELS_API_KEY}
    found   = []
    page    = 1

    while len(found) < max_photos:
        r = requests.get(
            "https://api.pexels.com/v1/search",
            headers=headers,
            params={"query": query, "per_page": 80, "page": page, "orientation": "landscape"}
        )
        if r.status_code != 200:
            print(f"  [Pexels] Erreur {r.status_code}")
            break

        data   = r.json()
        photos = data.get("photos", [])
        if not photos:
            break

        for p in photos:
            if (p["width"], p["height"]) in VALID_SIZES:
                found.append({
                    "id":  f"pexels_{p['id']}",
                    "url": p["src"]["original"],
                    "dim": f"{p['width']}x{p['height']}"
                })

        if not data.get("next_page"):
            break
        page += 1
        time.sleep(0.2)

    print(f"  [Pexels] {len(found)} images valides trouvées")
    return found[:max_photos]

# ─────────────────────────────────────────────
#  UNSPLASH
# ─────────────────────────────────────────────
def fetch_unsplash(query, max_photos):
    print(f"\n  [Unsplash] '{query}'")
    found = []
    page  = 1

    while len(found) < max_photos:
        r = requests.get(
            "https://api.unsplash.com/search/photos",
            params={
                "query":       query,
                "per_page":    30,
                "page":        page,
                "orientation": "landscape",
                "client_id":   UNSPLASH_API_KEY
            }
        )
        if r.status_code != 200:
            print(f"  [Unsplash] Erreur {r.status_code}")
            break

        results = r.json().get("results", [])
        if not results:
            break

        for p in results:
            w, h = p["width"], p["height"]
            if (w, h) in VALID_SIZES:
                found.append({
                    "id":  f"unsplash_{p['id']}",
                    "url": p["urls"]["raw"] + f"&w={w}&h={h}&fit=max&fm=jpg&q=100",
                    "dim": f"{w}x{h}"
                })

        page += 1
        time.sleep(0.2)

    print(f"  [Unsplash] {len(found)} images valides trouvées")
    return found[:max_photos]

# ─────────────────────────────────────────────
#  PIXABAY
# ─────────────────────────────────────────────
def fetch_pixabay(query, max_photos):
    print(f"\n  [Pixabay] '{query}'")
    found = []
    page  = 1

    while len(found) < max_photos:
        r = requests.get(
            "https://pixabay.com/api/",
            params={
                "key":         PIXABAY_API_KEY,
                "q":           query,
                "image_type":  "photo",
                "orientation": "horizontal",
                "per_page":    200,
                "page":        page,
                "safesearch":  "true"
            }
        )
        if r.status_code != 200:
            print(f"  [Pixabay] Erreur {r.status_code}")
            break

        hits = r.json().get("hits", [])
        if not hits:
            break

        for p in hits:
            w, h = p.get("imageWidth", 0), p.get("imageHeight", 0)
            if (w, h) in VALID_SIZES:
                url = p.get("largeImageURL") or p.get("webformatURL")
                found.append({
                    "id":  f"pixabay_{p['id']}",
                    "url": url,
                    "dim": f"{w}x{h}"
                })

        if len(hits) < 200:
            break
        page += 1
        time.sleep(0.2)

    print(f"  [Pixabay] {len(found)} images valides trouvées")
    return found[:max_photos]

# ─────────────────────────────────────────────
#  TÉLÉCHARGEMENT
# ─────────────────────────────────────────────
def download_all(query, max_per_source=50):
    folder = Path(OUTPUT_DIR) / query.replace(" ", "_")
    folder.mkdir(parents=True, exist_ok=True)
    Path(OUTPUT_DIR).mkdir(exist_ok=True)

    already = load_log()

    print(f"\n{'='*55}")
    print(f"  Recherche : {query}")
    print(f"  Déjà téléchargés (log) : {len(already)}")
    print(f"{'='*55}")

    sources = []
    if PEXELS_API_KEY:
        sources += fetch_pexels(query, max_per_source)
    if UNSPLASH_API_KEY:
        sources += fetch_unsplash(query, max_per_source)
    if PIXABAY_API_KEY:
        sources += fetch_pixabay(query, max_per_source)

    # Dédoublonnage + filtre log
    seen   = set()
    unique = []
    for img in sources:
        if img["id"] not in seen and img["id"] not in already:
            seen.add(img["id"])
            unique.append(img)

    print(f"\n📦 {len(unique)} nouvelles images à télécharger\n")

    downloaded = 0
    for img in unique:
        filename = folder / f"{img['id']}_{img['dim']}.jpg"
        try:
            r = requests.get(img["url"], stream=True, timeout=30)
            if r.status_code == 200:
                with open(filename, "wb") as f:
                    for chunk in r.iter_content(8192):
                        f.write(chunk)
                save_log(img["id"])
                downloaded += 1
                print(f"  ✅ [{downloaded}/{len(unique)}] {img['dim']} — {img['id']}")
                time.sleep(0.1)
            else:
                print(f"  ⚠️  HTTP {r.status_code} — {img['id']}")
        except Exception as e:
            print(f"  ⚠️  {e}")

    print(f"\n✅ {downloaded} nouveaux wallpapers dans '{folder}'")
    return downloaded

# ─────────────────────────────────────────────
#  MAIN
# ─────────────────────────────────────────────
def main():
    print("=" * 55)
    print("  Wallpaper Downloader — Mode aléatoire")
    print("  3840x2160 / 1920x1080 strict")
    print("=" * 55)

    # Pioche 3 thèmes maîtres au hasard
    masters = random.sample(list(THEMES.keys()), min(MASTER_THEMES, len(THEMES)))

    print(f"\n🎲 Thèmes maîtres tirés : {', '.join(masters)}\n")

    total = 0
    for master in masters:
        subs = THEMES[master]
        # Pioche 5 déclinaisons au hasard
        picked = random.sample(subs, min(SUB_PER_THEME, len(subs)))

        print(f"\n{'─'*55}")
        print(f"  🎨 {master.upper()}")
        print(f"  Déclinaisons : {', '.join(picked)}")
        print(f"{'─'*55}")

        for sub in picked:
            total += download_all(sub, max_per_source=50)

    print(f"\n{'='*55}")
    print(f"🎉 Terminé ! {total} nouveaux wallpapers au total")
    print(f"📁 Dossier : ./{OUTPUT_DIR}/")
    print(f"📋 Log : ./{LOG_FILE}")
    print(f"{'='*55}")

if __name__ == "__main__":
    main()