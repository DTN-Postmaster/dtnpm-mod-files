# Guide de Contribution — `dtnpm-mod-files`

Ce guide présente les règles de développement et les exigences de sécurité pour le module de transfert de fichiers de `dtnpm-mod-files`.

---

## 1. Principes Fondamentaux

- **Zéro Panic / Zéro Unwrap** : Aucun appel `.unwrap()` ou `.expect()` en code de production.
- **Sécurité du Stockage & Quotas** :
  - Toujours vérifier la taille du bundle avant de le charger en mémoire.
  - Toujours assainir le nom de fichier et borner sa longueur.
  - Toujours nettoyer le fichier sur disque si la transaction SQLite échoue.
- **Traçabilité Architecturale (ADR)** :
  - Tout changement dans la gestion du stockage ou de l'API doit faire l'objet d'un fichier ADR sous `adr/` et être indexé dans `adr/README.md`.

---

## 2. Vérification Locale

```bash
cargo fmt --all -- --check
cargo clippy --offline --workspace --lib --bins -- -D warnings -D clippy::unwrap_used -D clippy::panic
cargo test --offline --all-features
./scripts/check-security.sh --strict
```
