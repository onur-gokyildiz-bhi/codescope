# Codescope Roadmap — Rekabet Planı

> Rakip analizi + eksik özellik planı. Hedef: En iyi açık kaynak kod bilgi grafı aracı olmak.

---

## Rakip Analizi (Nisan 2026)

### 1. codebase-memory-mcp (DeusData)
- **Dil**: C, single static binary
- **Dil desteği**: 66 dil (biz: 19)
- **İndeksleme**: Linux kernel (28M LOC) 3 dakikada
- **3D Graf UI**: localhost:9749'da interaktif 3D görselleştirme
- **Otomatik watch**: Git değişikliklerini izleyip auto-reindex
- **Dead code detection**: Sıfır çağrıcısı olan fonksiyonları bulma
- **Cross-service HTTP linking**: REST route'ları HTTP çağrı sitelerine eşleme
- **ADR yönetimi**: Architecture Decision Record takibi
- **8 agent desteği**: Claude Code, Codex CLI, Gemini CLI, Zed, OpenCode, Antigravity, Aider, KiloCode
- **Eksikleri**: Embedding/semantic search yok, conversation memory yok, temporal analysis yok

### 2. Serena (Oraios)
- **Dil**: Python, LSP-tabanlı
- **40+ dil**: LSP serverleri üzerinden
- **Symbol-level ops**: Rename, move, inline, safe delete, replace body
- **JetBrains entegrasyonu**: IDE plugin ile gelişmiş refactoring
- **Memory sistemi**: Cross-session bilgi saklama
- **Eksikleri**: Ephemeral (her session sıfırdan), ağır (40+ process), embedding yok, graf yok

### 3. CodeGraphContext
- **Dil**: Python
- **14 dil**, KuzuDB/FalkorDB/Neo4j
- **Live file watcher**: Dosya değişikliğinde otomatik graf güncelleme
- **Esnek DB backend**: 4 farklı graf DB seçeneği
- **Eksikleri**: Embedding yok, single binary değil, conversation yok

### 4. SimpleMem
- **Dil**: Python
- **3 aşamalı pipeline**: Semantic compression → Online synthesis → Intent-aware retrieval
- **3 katmanlı indeks**: Vector + Lexical + Symbolic (metadata)
- **30x token tasarrufu**: ~550 token ile doğru sonuç
- **MCP server**: Claude Desktop, Cursor, LM Studio desteği
- **Eksikleri**: Kod analizi yok (sadece konuşma hafızası)

---

## Codescope'un Güçlü Yanları (Zaten Var)
- ✅ SurrealDB (graph + vector + document tek DB'de)
- ✅ Lokal embeddings (FastEmbed, sıfır dış bağımlılık)
- ✅ Conversation memory (karar/problem/çözüm takibi)
- ✅ Temporal analysis (git churn, hotspot, coupling)
- ✅ Obsidian-like navigation (explore, backlinks, context_bundle)
- ✅ Auto CONTEXT.md üretimi
- ✅ Incremental indexing (hash-based)
- ✅ Web UI (D3.js visualization)
- ✅ Single binary (Rust)

---

## Eksikler ve Öncelikli Geliştirmeler

### P0 — Kritik (Rekabetçi olmak için şart)

#### 1. File Watcher — Otomatik Re-index
**Rakip**: codebase-memory-mcp, CodeGraphContext
**Durum**: Yok — her seferinde manual index_codebase çağırılıyor
**Plan**: `notify` crate ile dosya değişikliklerini izle, debounce (2s) ile otomatik re-index
**Etki**: Kullanıcı deneyiminde büyük fark — "her zaman güncel"
**Efor**: Orta (2-3 saat)

#### 2. Daha Fazla Dil Desteği (19 → 40+)
**Rakip**: codebase-memory-mcp (66), Serena (40+)
**Durum**: 19 dil, ama Kotlin, R, Julia, Perl, PowerShell, Bash, Clojure vb. eksik
**Plan**: tree-sitter grammar'ları ekle (çoğu tek satır registry ekleme)
**Etki**: Daha geniş kullanıcı kitlesi
**Efor**: Düşük-orta (her dil ~15dk, toplam 3-4 saat)

#### 3. Dead Code Detection
**Rakip**: codebase-memory-mcp
**Durum**: Yok
**Plan**: `find_callers` sonucu boş olan fonksiyonları filtrele, entry point'leri (main, handler, test) hariç tut
**Etki**: Çok kullanışlı tool — temizlik için
**Efor**: Düşük (1 saat)

### P1 — Önemli (Fark yaratacak)

#### 4. SimpleMem Tarzı Conversation Compression
**Rakip**: SimpleMem (3 aşamalı pipeline)
**Durum**: Ham metin 500 char truncate
**Plan**:
  - Semantic compression: Önemli bilgiyi çıkar, gereksiz token'ları at
  - Online synthesis: Aynı konuyu tartışan segment'leri birleştir
  - 3 katmanlı indeks: Vector (mevcut) + keyword (mevcut) + metadata (timestamp ekle)
**Etki**: 30x token tasarrufu conversation retrieval'da
**Efor**: Yüksek (1-2 gün)

#### 5. Cross-Service HTTP Linking
**Rakip**: codebase-memory-mcp
**Durum**: Yok
**Plan**: REST endpoint'leri (axum/actix route'ları) parse et, HTTP client çağrılarını (reqwest) eşle
**Etki**: Microservice projelerde büyük değer
**Efor**: Orta (3-4 saat)

#### 6. Multi-Agent Desteği (8 agent)
**Rakip**: codebase-memory-mcp (8 agent)
**Durum**: Claude Code + Cursor (MCP üzerinden)
**Plan**: Codex CLI, Gemini CLI, Zed, OpenCode config template'leri ekle
**Etki**: Daha geniş erişim
**Efor**: Düşük (1-2 saat, sadece config template)

#### 7. Symbol-Level Operations
**Rakip**: Serena
**Durum**: Yok — sadece read-only analiz
**Plan**:
  - `rename_symbol`: Graf üzerinden tüm referansları bul, rename öner
  - `find_unused`: Dead code + unused import tespiti
  - `safe_delete`: Sıfır referanslı entity silme önerisi
**Etki**: Refactoring desteği — yazma yeteneği
**Efor**: Yüksek (2-3 gün)

### P2 — Nice-to-Have (Gelecek fazlar)

#### 8. 3D Graf Görselleştirme
**Rakip**: codebase-memory-mcp (Three.js)
**Durum**: 2D D3.js force-directed
**Plan**: Three.js veya Sigma.js ile 3D orbit view
**Efor**: Orta-yüksek

#### 9. ADR (Architecture Decision Record) Yönetimi
**Rakip**: codebase-memory-mcp
**Plan**: `manage_adr` tool — conversation'lardan çıkarılan kararları yapılandırılmış ADR formatına çevir
**Efor**: Orta

#### 10. Type Hierarchy / Interface Implementation Tracking
**Rakip**: Serena, CodeGraphContext
**Plan**: `type_hierarchy` tool — inherits/implements edge'lerini tam traverse et
**Efor**: Düşük-orta

---

## Uygulama Sırası

| Hafta | Özellik | Etki | Efor |
|-------|---------|------|------|
| 1 | File watcher (P0) | ⭐⭐⭐⭐⭐ | Orta |
| 1 | Dead code detection (P0) | ⭐⭐⭐⭐ | Düşük |
| 1 | Multi-agent config (P1) | ⭐⭐⭐ | Düşük |
| 2 | +20 dil desteği (P0) | ⭐⭐⭐⭐ | Orta |
| 2 | Cross-service HTTP linking (P1) | ⭐⭐⭐⭐ | Orta |
| 3 | Conversation compression (P1) | ⭐⭐⭐⭐⭐ | Yüksek |
| 4 | Symbol-level ops (P1) | ⭐⭐⭐⭐ | Yüksek |
| 5+ | 3D viz, ADR, type hierarchy (P2) | ⭐⭐⭐ | Değişken |

---

## Hedef: v0.3.0

Tüm P0 + P1 tamamlandığında:
- 40+ dil (şu an 19)
- File watcher ile otomatik güncelleme
- Dead code detection
- Cross-service HTTP linking
- SimpleMem tarzı conversation compression
- 8+ agent desteği
- Symbol-level rename/delete önerileri

**Sonuç**: Tek rakip avantajı kalan şey codebase-memory-mcp'nin C ile yazılmış olmasından gelen ham hız. Ama özellik seti olarak Codescope onun önünde olacak.
