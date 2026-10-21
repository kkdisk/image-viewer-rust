# 專案規範與 AI 指引 (AGENTS.md)

本專案為高效能純 Rust 桌面圖片瀏覽、編輯與查重工具。

## 核心規則 (Core Rules)
1. **語言要求**：所有輸出（包含回覆、思考、任務清單、分析、建議、程式碼註解）均使用繁體中文，嚴禁使用簡體或其他語言。
2. **全域擴充技能庫**：本系統已掛載全域擴充技能庫，軟體工程相關任務請優先前往絕對路徑 \D:\code\AI_SKILL\agent-skills\skills\\ 查看是否有對應的 \SKILL.md\ 並遵循執行。
3. **任務與進度追蹤**：所有任務進度必須嚴格參照並更新 \	asks/todo.md\ 與 \	asks/plan.md\。
4. **規格依循**：所有架構設計與邊界規則必須嚴格遵循 \docs/SPEC_RUST_IMAGE_VIEWER.md\。

## 技術棧 (Tech Stack)
- **Rust Edition**：2024
- **GUI 框架**：\gui\ + \frame\ (WGPU 硬體加速)
- **平行處理**：\ayon\ (CPU 密集像素運算與批次雜湊)
- **非同步 I/O**：\	okio\ (背景檔案載入與縮圖佇列)
- **影像解碼**：\image\ crate、\kamadak-exif\

## 常見開發指令
- \cargo check --workspace\：檢查所有 crate
- \cargo test --workspace\：執行所有單元測試
- \cargo run -p app-ui\：啟動桌面應用
- \cargo clippy --all-targets -- -D warnings\：靜態代碼檢查