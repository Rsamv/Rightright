// 控制台诊断程序：trtest.exe "文本" [目标语言Bing代码]
// 用于验证 Bing/Google 翻译接口是否可用，与 GUI 共用同一翻译逻辑。
use rightright::translate;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let text = args.get(1).cloned().unwrap_or_else(|| "Hello, world!".to_string());
    let to = args.get(2).cloned().unwrap_or_else(|| "zh-Hans".to_string());
    println!("输入: {text}\n目标: {to}");
    match translate::translate(&text, &to) {
        Ok(t) => println!("译文: {t}"),
        Err(e) => println!("失败: {e}"),
    }
}