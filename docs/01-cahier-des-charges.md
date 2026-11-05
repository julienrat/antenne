# 01 — Cahier des charges

> Statut : **brouillon v0.1** (5 novembre 2026).
> Légende : ✅ acquis (exprimé par Julien) · 🔶 hypothèse de départ, **à valider**.

## 1. Objectif

Antenne est un logiciel **tout-en-un en Rust** qui :

1. ✅ simule **plusieurs cartes son virtuelles** sur l'ordinateur ;
2. ✅ diffuse **chaque entrée** (chaque carte) sous forme de **flux audio unicast** (« webradio ») ;
3. ✅ privilégie la **faible latence** ;
4. ✅ alimente **plusieurs ESP32**, chacun écoutant **un** des flux ;
5. ✅ fonctionne sous **Linux, macOS et Windows** (Linux d'abord, puis portage).

Cas d'usage type : un logiciel (DAW, lecteur, mixeur…) joue vers « Antenne Carte 1 »,
« Antenne Carte 2 »… ; chaque carte part en temps réel vers un ESP32 différent (enceinte,
haut-parleur d'une pièce, retour de scène…). 🔶

## 2. Exigences fonctionnelles

| ID    | Exigence | Origine |
|-------|----------|---------|
| EF-01 | Créer N cartes son virtuelles, N défini dans la configuration | ✅ |
| EF-02 | Chaque carte est **mono ou stéréo** (configurable par carte) | ✅ |
| EF-03 | Chaque carte a son **encodage** configurable : Opus (défaut), G.711 µ/A-law, PCM16 | ✅ (liste 🔶) |
| EF-04 | Chaque carte a son **chemin / adresse WebRTC** configurable (ex. `/whep/carte1`) | ✅ |
| EF-05 | Diffusion **unicast** : une session par client, pas de multicast | ✅ |
| EF-06 | Plusieurs clients peuvent écouter la même carte, avec un plafond (`max_listeners`) | 🔶 |
| EF-07 | Une carte peut aussi capturer une **entrée existante** (micro, interface, bouclage) | 🔶 |
| EF-08 | Générateur de **tonalité de test** (aucun matériel requis) | 🔶 |
| EF-09 | **API d'état** JSON (`/api/cards`) : cartes, statut, auditeurs | 🔶 |
| EF-10 | **Flux de debug** HTTP WAV (`/stream/<id>`) écoutable dans VLC / ffplay | 🔶 |
| EF-11 | Commandes `check` (valider la config) et `devices` (lister les entrées) | 🔶 |
| EF-12 | *(plus tard)* Interface de configuration (web locale ou icône de barre) | 🔶 |

## 3. Exigences non fonctionnelles

| ID     | Exigence | Cible 🔶 |
|--------|----------|----------|
| ENF-01 | Latence « son joué sur le PC → son sorti de l'ESP32 », en Wi-Fi local | **≤ 100 ms**, objectif 50 ms |
| ENF-02 | Un seul exécutable par OS, sans runtime à installer (hors pilote de bouclage sur macOS / Windows) | — |
| ENF-03 | Une carte en panne n'arrête pas les autres ; un client qui se déconnecte n'affecte pas les autres | — |
| ENF-04 | Aucun décrochage (xrun) avec 8 cartes stéréo sur une machine courante | à mesurer |
| ENF-05 | Réseau local uniquement en v1 (pas d'authentification, pas de traversée de NAT) | à décider |
| ENF-06 | Configuration en un fichier TOML lisible, validée au démarrage avec des messages clairs | — |

### Budget de latence indicatif (ENF-01)

| Étape | Estimation |
|-------|-----------:|
| Tampon de capture (cpal / PipeWire / CoreAudio / WASAPI) | 5–20 ms |
| Découpage en trames (`frame_ms`) | 10 ms |
| Encodage Opus 10 ms | < 1 ms |
| Réseau Wi-Fi local | 2–10 ms |
| Tampon de gigue côté ESP32 | 20–40 ms |
| Tampon I2S / DAC ESP32 | 5–10 ms |
| **Total** | **≈ 45–90 ms** |

Ces chiffres sont des ordres de grandeur, à remplacer par des mesures (jalon 2).

## 4. Contraintes de plateforme (résumé)

Détails dans [03-plateformes.md](03-plateformes.md).

- **Linux** : Antenne crée elle-même ses cartes (null sinks PipeWire). Rien à installer d'autre.
- **macOS** : créer un périphérique audio demande un pilote (AudioServerPlugIn). v1 s'appuie sur
  **BlackHole** (16 canaux) et découpe ses canaux en cartes.
- **Windows** : idem avec un pilote existant (**VB-CABLE**, Virtual Audio Cable…). Un pilote
  maison impose une signature Microsoft : hors périmètre v1.

## 5. Hors périmètre v1

- Diffusion via Internet, traversée de NAT, serveurs TURN.
- Chiffrement / authentification au-delà de ce qu'impose WebRTC (DTLS-SRTP).
- Pilotes audio maison macOS / Windows.
- Synchronisation fine entre plusieurs ESP32 (lecture « en phase »).
- Firmware ESP32 complet (seul un prototype de client est prévu, jalon 4).

## 6. Questions ouvertes (à trancher par Julien)

1. Combien de cartes au maximum, et combien d'auditeurs par carte ?
2. Quels ESP32 exactement (ESP32 classique, S3, P4 ?), avec quel DAC / ampli (I2S) ?
3. WebRTC est-il imposé, ou un flux RTP/UDP plus simple suffirait-il pour les ESP32 ?
4. Faut-il que plusieurs ESP32 jouent **en phase** (même pièce) ?
5. Usage commercial prévu (micro-entreprise) ? → impacte la licence d'Antenne et le choix des
   pilotes tiers (BlackHole est sous GPL-3.0, VB-CABLE est en « donationware »).
6. Interface graphique souhaitée, et laquelle (page web locale, application, icône de barre) ?
