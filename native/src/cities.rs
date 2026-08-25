//! Short city list + district/county search — mirrors `src/services/cities.ts`.

use serde::Deserialize;
use std::time::Duration;

#[path = "districts.rs"]
mod districts;

#[derive(Clone, Debug)]
pub struct CityOption {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub region: Option<String>,
}

impl CityOption {
    pub fn display_name(&self) -> String {
        let name = self.name.trim();
        let region = self
            .region
            .as_deref()
            .map(short_admin)
            .filter(|s| !s.is_empty());
        match region {
            Some(r) if r != name && !name.contains(&r) && !r.contains(name) => {
                format!("{name} · {r}")
            }
            _ => name.to_string(),
        }
    }
}

/// Capitals + common prefecture cities (incl. all Guangxi prefectures).
const COMMON: &[(&str, f64, f64, Option<&str>)] = &[
    ("北京", 39.9042, 116.4074, None),
    ("天津", 39.3434, 117.3616, None),
    ("上海", 31.2304, 121.4737, None),
    ("重庆", 29.5630, 106.5516, None),
    ("石家庄", 38.0428, 114.5149, Some("河北")),
    ("太原", 37.8706, 112.5489, Some("山西")),
    ("呼和浩特", 40.8426, 111.7492, Some("内蒙古")),
    ("沈阳", 41.8057, 123.4315, Some("辽宁")),
    ("大连", 38.9140, 121.6147, Some("辽宁")),
    ("长春", 43.8171, 125.3235, Some("吉林")),
    ("哈尔滨", 45.8038, 126.5350, Some("黑龙江")),
    ("南京", 32.0603, 118.7969, Some("江苏")),
    ("苏州", 31.2989, 120.5853, Some("江苏")),
    ("无锡", 31.4912, 120.3119, Some("江苏")),
    ("常州", 31.8107, 119.9741, Some("江苏")),
    ("南通", 32.0162, 120.8946, Some("江苏")),
    ("扬州", 32.3932, 119.4129, Some("江苏")),
    ("徐州", 34.2058, 117.2841, Some("江苏")),
    ("杭州", 30.2741, 120.1551, Some("浙江")),
    ("宁波", 29.8683, 121.5440, Some("浙江")),
    ("温州", 28.0006, 120.6994, Some("浙江")),
    ("嘉兴", 30.7539, 120.7585, Some("浙江")),
    ("金华", 29.0788, 119.6474, Some("浙江")),
    ("绍兴", 30.0023, 120.5819, Some("浙江")),
    ("合肥", 31.8206, 117.2272, Some("安徽")),
    ("福州", 26.0745, 119.2965, Some("福建")),
    ("厦门", 24.4798, 118.0894, Some("福建")),
    ("泉州", 24.8740, 118.6757, Some("福建")),
    ("南昌", 28.6820, 115.8581, Some("江西")),
    ("济南", 36.6512, 117.1201, Some("山东")),
    ("青岛", 36.0671, 120.3826, Some("山东")),
    ("烟台", 37.4638, 121.4479, Some("山东")),
    ("潍坊", 36.7069, 119.1619, Some("山东")),
    ("郑州", 34.7466, 113.6254, Some("河南")),
    ("洛阳", 34.6197, 112.4540, Some("河南")),
    ("武汉", 30.5928, 114.3055, Some("湖北")),
    ("长沙", 28.2282, 112.9388, Some("湖南")),
    ("广州", 23.1291, 113.2644, Some("广东")),
    ("深圳", 22.5431, 114.0579, Some("广东")),
    ("佛山", 23.0215, 113.1214, Some("广东")),
    ("东莞", 23.0207, 113.7518, Some("广东")),
    ("珠海", 22.2710, 113.5767, Some("广东")),
    ("中山", 22.5159, 113.3928, Some("广东")),
    ("惠州", 23.1115, 114.4152, Some("广东")),
    ("汕头", 23.3541, 116.6820, Some("广东")),
    ("南宁", 22.8170, 108.3669, Some("广西")),
    ("柳州", 24.3264, 109.4281, Some("广西")),
    ("桂林", 25.2736, 110.2900, Some("广西")),
    ("梧州", 23.4769, 111.2970, Some("广西")),
    ("北海", 21.4733, 109.1200, Some("广西")),
    ("防城港", 21.6867, 108.3547, Some("广西")),
    ("钦州", 21.9500, 108.6194, Some("广西")),
    ("贵港", 23.0936, 109.6096, Some("广西")),
    ("玉林", 22.6293, 110.1545, Some("广西")),
    ("百色", 23.9023, 106.6186, Some("广西")),
    ("贺州", 24.4141, 111.5520, Some("广西")),
    ("河池", 24.6929, 108.0850, Some("广西")),
    ("来宾", 23.7338, 109.2292, Some("广西")),
    ("崇左", 22.4167, 107.3649, Some("广西")),
    ("海口", 20.0440, 110.1990, Some("海南")),
    ("三亚", 18.2528, 109.5117, Some("海南")),
    ("成都", 30.5728, 104.0668, Some("四川")),
    ("绵阳", 31.4678, 104.6791, Some("四川")),
    ("贵阳", 26.6470, 106.6302, Some("贵州")),
    ("昆明", 25.0389, 102.7183, Some("云南")),
    ("大理", 25.6065, 100.2676, Some("云南")),
    ("丽江", 26.8550, 100.2270, Some("云南")),
    ("拉萨", 29.6520, 91.1721, Some("西藏")),
    ("西安", 34.3416, 108.9398, Some("陕西")),
    ("兰州", 36.0611, 103.8343, Some("甘肃")),
    ("西宁", 36.6171, 101.7782, Some("青海")),
    ("银川", 38.4872, 106.2309, Some("宁夏")),
    ("乌鲁木齐", 43.8256, 87.6168, Some("新疆")),
    ("香港", 22.3193, 114.1694, None),
    ("澳门", 22.1987, 113.5439, None),
    ("台北", 25.0330, 121.5654, None),
];

pub fn common_cities() -> Vec<CityOption> {
    COMMON
        .iter()
        .map(|(name, lat, lon, region)| CityOption {
            name: (*name).into(),
            latitude: *lat,
            longitude: *lon,
            region: region.map(|s| s.into()),
        })
        .collect()
}

pub fn filter_common(query: &str) -> Vec<CityOption> {
    let q = normalize_admin(query.trim());
    if q.is_empty() {
        return common_cities();
    }
    common_cities()
        .into_iter()
        .filter(|c| {
            normalize_admin(&c.name).contains(&q)
                || c.region
                    .as_ref()
                    .is_some_and(|r| normalize_admin(r).contains(&q) || q.contains(&normalize_admin(r)))
        })
        .collect()
}

fn district_cities() -> Vec<CityOption> {
    districts::DISTRICTS
        .iter()
        .map(|(name, lat, lon, city, _province)| CityOption {
            name: (*name).into(),
            latitude: *lat,
            longitude: *lon,
            region: Some((*city).into()),
        })
        .collect()
}

/// Match by district name, or list a city's districts when the query is that city.
fn filter_districts(query: &str) -> Vec<CityOption> {
    let q = normalize_admin(query.trim());
    if q.is_empty() || q.chars().count() < 2 || is_province_name(&q) {
        return Vec::new();
    }
    district_cities()
        .into_iter()
        .filter(|c| {
            let name = normalize_admin(&c.name);
            let city = c
                .region
                .as_deref()
                .map(normalize_admin)
                .unwrap_or_default();
            name.contains(&q) || q.contains(&name) || city == q
        })
        .collect()
}

#[derive(Deserialize)]
struct GeoResponse {
    results: Option<Vec<GeoHit>>,
}

#[derive(Deserialize)]
struct GeoHit {
    name: String,
    latitude: f64,
    longitude: f64,
    admin1: Option<String>,
    admin2: Option<String>,
    country_code: Option<String>,
    feature_code: Option<String>,
    population: Option<u32>,
}

/// Local cities/districts first; CJK queries use Photon (Open-Meteo misses 区县).
pub fn search_cities(query: &str) -> Vec<CityOption> {
    let q = query.trim();
    if q.is_empty() {
        return Vec::new();
    }
    let local = filter_common(q);
    let districts = filter_districts(q);
    let remote = search_remote(q).unwrap_or_default();

    let mut seen = std::collections::HashSet::new();
    let mut merged = Vec::new();
    for c in local.into_iter().chain(districts).chain(remote) {
        let key = format!("{}|{:.2}", normalize_admin(&c.name), c.latitude);
        if !seen.insert(key) {
            continue;
        }
        merged.push(c);
        if merged.len() >= 24 {
            break;
        }
    }
    merged
}

fn http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(8))
        .build()
}

fn search_remote(q: &str) -> Option<Vec<CityOption>> {
    if is_cjk(q) {
        let photon = search_photon(q).unwrap_or_default();
        if !photon.is_empty() {
            return Some(photon);
        }
    }
    search_open_meteo(q)
}

#[derive(Deserialize)]
struct PhotonResponse {
    features: Option<Vec<PhotonFeature>>,
}

#[derive(Deserialize)]
struct PhotonFeature {
    properties: PhotonProps,
    geometry: PhotonGeom,
}

#[derive(Deserialize)]
struct PhotonProps {
    name: Option<String>,
    city: Option<String>,
    state: Option<String>,
    countrycode: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    osm_type: Option<String>,
    osm_key: Option<String>,
    osm_value: Option<String>,
}

#[derive(Deserialize)]
struct PhotonGeom {
    coordinates: Vec<f64>,
}

fn search_photon(q: &str) -> Option<Vec<CityOption>> {
    let url = format!(
        "https://photon.komoot.io/api/?q={}&limit=12",
        url_encode(q)
    );
    let resp = http_agent()
        .get(&url)
        .set("User-Agent", "WeatherBall/0.2")
        .call()
        .ok()?;
    if !(200..300).contains(&resp.status()) {
        return None;
    }
    let data: PhotonResponse = resp.into_json().ok()?;
    let mut hits = data.features.unwrap_or_default();
    hits.retain(|f| keep_photon(q, f));
    hits.sort_by_key(|f| rank_photon(q, f));
    Some(
        hits.into_iter()
            .filter_map(|f| photon_to_city(f))
            .collect(),
    )
}

fn photon_to_city(f: PhotonFeature) -> Option<CityOption> {
    let lon = *f.geometry.coordinates.first()?;
    let lat = *f.geometry.coordinates.get(1)?;
    let name = f.properties.name.filter(|s| !s.is_empty())?;
    let region = f
        .properties
        .city
        .filter(|s| !s.is_empty())
        .or(f.properties.state.filter(|s| !s.is_empty()))
        .map(|s| short_admin(&s))
        .filter(|s| !s.is_empty() && s != &normalize_admin(&name));
    Some(CityOption {
        name,
        latitude: lat,
        longitude: lon,
        region,
    })
}

fn keep_photon(q: &str, f: &PhotonFeature) -> bool {
    let p = &f.properties;
    let name = p.name.as_deref().unwrap_or("");
    if name.is_empty() {
        return false;
    }
    let cc = p.countrycode.as_deref().unwrap_or("");
    if is_cjk(q) && !matches!(cc, "CN" | "HK" | "MO" | "TW" | "") {
        return false;
    }
    let kind = p.kind.as_deref().unwrap_or("");
    let osm_key = p.osm_key.as_deref().unwrap_or("");
    let osm_val = p.osm_value.as_deref().unwrap_or("");
    if matches!(
        osm_key,
        "amenity" | "highway" | "railway" | "office" | "landuse" | "shop" | "tourism" | "leisure"
    ) {
        return false;
    }
    if matches!(osm_val, "village" | "hamlet" | "isolated_dwelling") {
        return false;
    }
    let admin = matches!(kind, "city" | "district" | "county" | "town" | "state")
        || (osm_key == "place"
            && matches!(
                osm_val,
                "city"
                    | "district"
                    | "county"
                    | "town"
                    | "suburb"
                    | "municipality"
                    | "city_district"
            ))
        || is_admin_name(name);
    if !admin {
        return false;
    }
    let qn = normalize_admin(q);
    let nn = normalize_admin(name);
    let city = p.city.as_deref().map(normalize_admin).unwrap_or_default();
    let state = p.state.as_deref().map(normalize_admin).unwrap_or_default();
    if !nn.contains(&qn) && !city.contains(&qn) && !state.contains(&qn) && !qn.contains(&nn) {
        return false;
    }
    if is_province_name(&nn) && !state.is_empty() && !admin_matches_province(&state, &nn) {
        return false;
    }
    true
}

fn rank_photon(q: &str, f: &PhotonFeature) -> u8 {
    let p = &f.properties;
    let name = normalize_admin(p.name.as_deref().unwrap_or(""));
    let qn = normalize_admin(q);
    let kind = p.kind.as_deref().unwrap_or("");
    let osm_type = p.osm_type.as_deref().unwrap_or("");
    let mut r = 8u8;
    if matches!(kind, "district" | "county") {
        r = 1;
    } else if kind == "city" {
        r = 2;
    } else if kind == "town" {
        r = 4;
    }
    if osm_type == "R" {
        r = r.saturating_sub(1);
    }
    if name == qn || name == format!("{qn}区") || name == format!("{qn}县") {
        r = r.saturating_sub(1);
    }
    r
}

fn search_open_meteo(q: &str) -> Option<Vec<CityOption>> {
    let url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=20&language=zh&format=json",
        url_encode(q)
    );
    let resp = http_agent().get(&url).call().ok()?;
    if !(200..300).contains(&resp.status()) {
        return None;
    }
    let data: GeoResponse = resp.into_json().ok()?;
    let mut hits = data.results.unwrap_or_default();
    hits.retain(|h| keep_hit(q, h));
    hits.sort_by_key(|h| rank_hit(q, h));
    Some(
        hits.into_iter()
            .map(|r| CityOption {
                name: r.name,
                latitude: r.latitude,
                longitude: r.longitude,
                region: r
                    .admin1
                    .filter(|s| !s.is_empty())
                    .or(r.admin2.filter(|s| !s.is_empty())),
            })
            .collect(),
    )
}

fn keep_hit(q: &str, h: &GeoHit) -> bool {
    let qn = normalize_admin(q);
    let name = normalize_admin(&h.name);
    let admin1 = h.admin1.as_deref().map(normalize_admin).unwrap_or_default();
    let admin2 = h.admin2.as_deref().map(normalize_admin).unwrap_or_default();
    let country = h.country_code.as_deref().unwrap_or("");
    let feat = h.feature_code.as_deref().unwrap_or("");
    let pop = h.population.unwrap_or(0);
    let cjk = is_cjk(q);

    if cjk && !matches!(country, "CN" | "HK" | "MO" | "TW" | "") {
        return false;
    }
    if !name.contains(&qn) && !admin1.contains(&qn) && !admin2.contains(&qn) && !qn.contains(&name)
    {
        return false;
    }
    // "广西" as a Jiangsu village — name is a province, but admin1 is another province.
    if is_province_name(&name) && !admin1.is_empty() && !admin_matches_province(&admin1, &name) {
        return false;
    }
    if feat == "PPL" && pop < 80_000 && !is_admin_name(&h.name) {
        return false;
    }
    if (feat.starts_with("PPLX") || feat == "PPLQ") && !is_admin_name(&h.name) {
        return false;
    }
    true
}

fn rank_hit(q: &str, h: &GeoHit) -> u8 {
    let qn = normalize_admin(q);
    let name = normalize_admin(&h.name);
    let admin1 = h.admin1.as_deref().map(normalize_admin).unwrap_or_default();
    let feat = h.feature_code.as_deref().unwrap_or("");
    let country = h.country_code.as_deref().unwrap_or("");
    let mut r = 10u8;
    if matches!(feat, "PPLC" | "ADM1") {
        r = 0;
    } else if matches!(feat, "PPLA" | "ADM2" | "ADM3") {
        r = 1;
    } else if feat == "PPLA2" || is_admin_name(&h.name) {
        r = 2;
    } else if feat.starts_with("PPL") {
        r = 5;
    }
    if country == "CN" || country == "HK" || country == "MO" || country == "TW" {
        r = r.saturating_sub(0);
    } else {
        r = r.saturating_add(4);
    }
    if name == qn {
        r = r.saturating_sub(1);
    }
    if admin1.contains(&qn) {
        r = r.saturating_sub(1);
    }
    r
}

fn is_admin_name(name: &str) -> bool {
    name.ends_with('区')
        || name.ends_with('县')
        || name.ends_with('旗')
        || name.ends_with("自治县")
        || name.ends_with("自治旗")
        || name.ends_with("新区")
}

fn is_province_name(name: &str) -> bool {
    const PROVINCES: &[&str] = &[
        "河北", "山西", "辽宁", "吉林", "黑龙江", "江苏", "浙江", "安徽", "福建", "江西",
        "山东", "河南", "湖北", "湖南", "广东", "海南", "四川", "贵州", "云南", "陕西",
        "甘肃", "青海", "台湾", "内蒙古", "广西", "西藏", "宁夏", "新疆",
        "北京", "天津", "上海", "重庆", "香港", "澳门",
    ];
    PROVINCES.contains(&name)
}

fn admin_matches_province(admin: &str, province: &str) -> bool {
    admin == province || admin.contains(province) || province.contains(admin)
}

fn short_admin(s: &str) -> String {
    normalize_admin(s)
}

fn normalize_admin(s: &str) -> String {
    s.replace('\u{00a0}', " ")
        .replace("特别行政区", "")
        .replace("维吾尔自治区", "")
        .replace("壮族自治区", "")
        .replace("回族自治区", "")
        .replace("自治区", "")
        .replace('省', "")
        .replace('市', "")
        .trim()
        .to_string()
}

fn is_cjk(s: &str) -> bool {
    s.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
