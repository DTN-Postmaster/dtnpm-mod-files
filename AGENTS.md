# Agent Guide — `dtnpm-mod-files`

## 🎯 Mission
Fournir le module applicatif complet (Fullstack WebModule) de transfert et stockage de fichiers DTN pour **DTN Postmaster**, comprenant le service BPA (`FilesService`), la gestion des quotas disque, la persistance SQLite, l'API REST `/api/files/v1/...` et le composant d'interface Preact `<FilesView />`.

---

## 🛡️ Référentiels et Exigences de Sécurité

### 1. Cyber Resilience Act (CRA) & BPSec (RFC 9172)
- **Vérification BPSec Stricte par Défaut** :
  - Sous `VerifyPolicy::Strict`, tout fichier entrant sans signature valide (`Unsigned`, `MissingKey`, `Invalid`) est immédiatement rejeté avant tout stockage physique sur le disque.
- **Bornage de la Durée de Vie des Bundles** :
  - Plafonnement arithmétique de la durée de vie des bundles pour éviter tout débordement `u64`.
- **Supply-Chain & VEX** :
  - Audit `cargo-audit` et `cargo-deny` vérifié ; VEX `not_affected` pour `RUSTSEC-2026-0459`.
  - Actions CI épinglées par SHA-1 à 40 caractères.
- **Politique de sécurité** : Consignée dans [SECURITY.md](SECURITY.md).

### 2. OWASP & Sécurité du Stockage de Fichiers
- **Protection Pré-Quota contre la Saturation RAM** :
  - Vérification de `total_len` par rapport au quota restant AVANT d'appeler `buffer_stream` : tout transfert excédant la limite est rejeté en flux sans allouer de mémoire tampon géante.
- **Assainissement des Noms de Fichiers & Évitement de Traversée** :
  - Assainissement strict du nom de fichier : caractères restreints à `[A-Za-z0-9_-]` et longueur bornée à 128 caractères maximum (prévention de `ENAMETOOLONG` et des traversées de répertoires `../`).
- **Écritures Asynchrones & Suppression des Fichiers Orphelins** :
  - Les écritures physiques utilisent `tokio::fs::write` pour ne jamais bloquer l'exécuteur asynchrone Tokio.
  - En cas d'échec d'insertion dans la base de métadonnées SQLite, le fichier temporaire écrit sur le disque est immédiatement supprimé pour éviter l'accumulation d'orphelins.

---

## 🦀 Standards d'Ingénierie Rust

1. **Règle Zéro Panic / Zéro Unwrap** :
   - Tout le code sous `src/` doit être 100% exempt de `.unwrap()`, `.expect()` et de macro `panic!`.
   - Linter strict : `-D warnings -D clippy::unwrap_used -D clippy::panic`.
2. **ADRs Obligatoires** :
   - Toute décision architecturale Fichiers/BPSec doit être consignée sous `adr/` et indexée dans [adr/README.md](adr/README.md).

---

## ⚡ Commandes Critiques de Vérification

- `cargo fmt --all -- --check`
- `cargo clippy --offline --workspace --lib --bins -- -D warnings -D clippy::unwrap_used -D clippy::panic`
- `cargo test --offline --all-features`
- `./scripts/check-security.sh --strict`
- `./scripts/check-coverage.sh --summary-only`
