use crate::config;
use regex::Regex;
use serde_json::Value;
use std::time::Duration;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
(KHTML, like Gecko) Chrome/124.0 Safari/537.36";

type Backend = fn(&str, &str) -> Result<String, String>;

/// 翻译入口：有道 demo 优先（国内直连、自动识别语种），回退 MyMemory / Bing / Google。
pub fn translate(text: &str, target_bing: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("空文本".into());
    }
    let text = if text.chars().count() > 4800 {
        text.chars().take(4800).collect::<String>()
    } else {
        text.to_string()
    };

    let backends: [(&str, Backend); 4] = [
        ("有道", youdao),
        ("MyMemory", mymemory),
        ("Bing", bing),
        ("Google", google),
    ];
    let mut errs: Vec<String> = Vec::new();
    for (name, f) in backends {
        match f(&text, target_bing) {
            Ok(s) if !s.trim().is_empty() => return Ok(s),
            Ok(_) => errs.push(format!("{name}: 返回空")),
            Err(e) => errs.push(format!("{name}: {e}")),
        }
    }
    Err(errs.join("；"))
}

fn agent() -> Result<ureq::Agent, String> {
    let conn = native_tls::TlsConnector::new().map_err(|e| format!("TLS 初始化失败: {e}"))?;
    Ok(ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(12))
        .user_agent(UA)
        .tls_connector(std::sync::Arc::new(conn))
        .build())
}

/// 有道 demo：from=Auto 自动识别，translation 为译文数组
fn youdao(text: &str, to: &str) -> Result<String, String> {
    let agent = agent()?;
    let resp = agent
        .post("https://aidemo.youdao.com/trans")
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_form(&[("q", text), ("from", "Auto"), ("to", config::to_youdao_code(to))])
        .map_err(|e| format!("请求失败: {e}"))?;
    let body = resp.into_string().map_err(|e| format!("读结果失败: {e}"))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("解析失败: {e}"))?;
    let code = v.get("errorCode").and_then(|x| x.as_str()).unwrap_or("0");
    if code != "0" {
        return Err(format!("errorCode {code}"));
    }
    let arr = v.get("translation").and_then(|x| x.as_array()).ok_or("无 translation")?;
    let out: String = arr.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join("");
    if out.is_empty() {
        Err("返回空".into())
    } else {
        Ok(out)
    }
}

/// MyMemory：需显式源语言，按字符集粗判
fn mymemory(text: &str, to: &str) -> Result<String, String> {
    let src = guess_source(text);
    let pair = format!("{src}|{}", config::to_mymemory_code(to));
    let agent = agent()?;
    let resp = agent
        .get("https://api.mymemory.translated.net/get")
        .query("q", text)
        .query("langpair", &pair)
        .call()
        .map_err(|e| format!("请求失败: {e}"))?;
    let body = resp.into_string().map_err(|e| format!("读结果失败: {e}"))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("解析失败: {e}"))?;
    let out = v
        .get("responseData")
        .and_then(|x| x.get("translatedText"))
        .and_then(|x| x.as_str())
        .unwrap_or("");
    if out.is_empty() {
        Err("返回空".into())
    } else {
        Ok(out.to_string())
    }
}

fn guess_source(text: &str) -> &'static str {
    for c in text.chars() {
        let u = c as u32;
        if (0x3040..=0x30FF).contains(&u) {
            return "ja";
        }
        if (0xAC00..=0xD7AF).contains(&u) {
            return "ko";
        }
        if (0x4E00..=0x9FFF).contains(&u) {
            return "zh-CN";
        }
    }
    "en"
}

fn bing(text: &str, to: &str) -> Result<String, String> {
    let agent = agent()?;
    let resp = agent
        .get("https://cn.bing.com/translator")
        .set("Referer", "https://cn.bing.com/")
        .call()
        .map_err(|e| format!("取页面失败: {e}"))?;

    let cookies = collect_cookies(&resp);
    let html = resp.into_string().map_err(|e| format!("读页面失败: {e}"))?;

    let ig = Regex::new(r#"IG:"([0-9A-Fa-f]+)""#)
        .unwrap()
        .captures(&html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .ok_or("未找到 IG")?;

    let iid = Regex::new(r#"data-iid="([^"]+)""#)
        .unwrap()
        .captures(&html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "translator.5024".to_string());

    let helper = Regex::new(r#"params_AbusePreventionHelper\s*=\s*\[(\d+),"([^"]+)""#)
        .unwrap()
        .captures(&html)
        .ok_or("未找到 token/key")?;
    let key = helper.get(1).unwrap().as_str().to_string();
    let token = helper.get(2).unwrap().as_str().to_string();

    // 2) 提交翻译
    let url = format!(
        "https://cn.bing.com/ttranslatev3?isVertical=1&IG={ig}&IID={iid}.1"
    );
    let mut req = agent
        .post(&url)
        .set("Content-Type", "application/x-www-form-urlencoded")
        .set("Referer", "https://cn.bing.com/translator")
        .set("Origin", "https://cn.bing.com");
    if !cookies.is_empty() {
        req = req.set("Cookie", &cookies);
    }
    let resp = req
        .send_form(&[
            ("fromLang", "auto-detect"),
            ("to", to),
            ("text", text),
            ("token", token.as_str()),
            ("key", key.as_str()),
        ])
        .map_err(|e| format!("提交失败: {e}"))?;

    let body = resp.into_string().map_err(|e| format!("读结果失败: {e}"))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("解析失败: {e}"))?;
    v.get(0)
        .and_then(|x| x.get("translations"))
        .and_then(|t| t.get(0))
        .and_then(|t| t.get("text"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("结果无译文: {}", truncate(&body, 160)))
}

fn google(text: &str, target_bing: &str) -> Result<String, String> {
    let tl = config::to_google_code(target_bing);
    let agent = agent()?;
    let resp = agent
        .get("https://translate.googleapis.com/translate_a/single")
        .query("client", "gtx")
        .query("sl", "auto")
        .query("tl", tl)
        .query("dt", "t")
        .query("q", text)
        .call()
        .map_err(|e| format!("请求失败: {e}"))?;
    let body = resp.into_string().map_err(|e| format!("读结果失败: {e}"))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("解析失败: {e}"))?;
    let segs = v.get(0).and_then(|x| x.as_array()).ok_or("结果格式异常")?;
    let mut out = String::new();
    for seg in segs {
        if let Some(s) = seg.get(0).and_then(|x| x.as_str()) {
            out.push_str(s);
        }
    }
    if out.is_empty() {
        Err("Google 返回空".into())
    } else {
        Ok(out)
    }
}

fn collect_cookies(resp: &ureq::Response) -> String {
    let mut pairs = Vec::new();
    for c in resp.all("set-cookie") {
        if let Some(first) = c.split(';').next() {
            let first = first.trim();
            if first.contains('=') {
                pairs.push(first.to_string());
            }
        }
    }
    pairs.join("; ")
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        s.chars().take(n).collect::<String>() + "…"
    } else {
        s.to_string()
    }
}