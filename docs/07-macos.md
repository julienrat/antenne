# 07 — Utiliser Antenne sur macOS

Ce guide va de l'installation à la diffusion vers les ESP32, sur un Mac. Pour l'interface web
elle-même, voir [06-interface.md](06-interface.md).

## En bref

1. Installer **BlackHole** (le pilote qui fournit les « cartes son virtuelles »).
2. Installer **Antenne.app** depuis le DMG, l'ouvrir une première fois avec **clic droit > Ouvrir**.
3. Accepter les autorisations demandées : **Terminal**, **micro**, **connexions entrantes**.
4. Envoyer le son des applications vers les **canaux de BlackHole** correspondant à chaque carte.
5. Vérifier dans l'interface (<http://localhost:8080/>) que les vu-mètres bougent, puis
   brancher les ESP32.

## Comment ça marche sur Mac

Sous Linux, Antenne crée elle-même une vraie sortie audio par carte. **macOS ne le permet pas
sans pilote système** : Antenne s'appuie donc sur **BlackHole**, un pilote gratuit qui crée une
carte son virtuelle à plusieurs canaux. Antenne **découpe ses canaux en cartes** :

```
Application A ──► BlackHole canaux 1-2 ──► carte 1 « Salon »   ──► ESP32 salon
Application B ──► BlackHole canaux 3-4 ──► carte 2 « Cuisine » ──► ESP32 cuisine
Application C ──► BlackHole canal 5    ──► carte 3 « Annonces » (mono)
```

Conséquence importante : **chaque application doit envoyer son son sur les bons canaux de
BlackHole**. C'est l'étape « Router le son » plus bas.

## 1. Prérequis

- macOS 11 (Big Sur) ou plus récent, Mac Apple Silicon ou Intel.
- Le DMG `Antenne-<version>-macos-universal.dmg`, sur la
  [page des releases](https://github.com/julienrat/antenne/releases).
- **BlackHole**, en choisissant la variante selon le nombre de cartes :

| Variante | Cartes stéréo | Cartes mono | Conseil |
|----------|---------------|-------------|---------|
| BlackHole 2ch | 1 | 2 | Pour essayer avec une seule carte. |
| **BlackHole 16ch** | **8** | **16** | **Recommandé** : réglage par défaut d'Antenne. |
| BlackHole 64ch | 32 | 64 | Seulement au-delà de 8 cartes stéréo : un peu plus gourmand, chaque carte lisant les 64 canaux. |

Les cartes peuvent mélanger stéréo (2 canaux) et mono (1 canal) : il faut simplement que le
total des canaux tienne dans le pilote.

## 2. Installer BlackHole

Avec [Homebrew](https://brew.sh) :

```sh
brew install --cask blackhole-16ch      # ou blackhole-2ch, blackhole-64ch
```

Sans Homebrew : télécharger l'installeur sur <https://existential.audio/blackhole/>
(une adresse e-mail est demandée), puis l'ouvrir.

Vérifier : **Configuration audio et MIDI** (dans Applications > Utilitaires) doit lister
« BlackHole 16ch ».

> BlackHole est distribué sous licence GPL-3.0 par Existential Audio. Antenne ne l'embarque
> pas : il l'utilise comme n'importe quelle carte son.

## 3. Installer Antenne

1. Ouvrir le DMG et glisser **Antenne.app** dans **Applications**.
2. **Premier lancement** : dans Applications, **clic droit sur Antenne.app > Ouvrir**, puis
   **Ouvrir** dans la boîte de dialogue. L'application n'est pas encore notarisée par Apple :
   un double-clic simple affiche « impossible de vérifier le développeur ».
   - Si macOS refuse quand même : **Réglages Système > Confidentialité et sécurité**, tout en
     bas, bouton **Ouvrir quand même** à côté du message sur Antenne.
   - En dernier recours, dans le Terminal :
     `xattr -dr com.apple.quarantine /Applications/Antenne.app`
3. Les lancements suivants se font normalement (double-clic, Launchpad, Spotlight).

## 4. Premier lancement et autorisations

Antenne n'a pas de fenêtre à elle : elle **s'ouvre dans une fenêtre du Terminal**, qui affiche
son journal, et **ouvre l'interface dans le navigateur**. Au premier lancement, macOS pose
plusieurs questions ; il faut toutes les accepter :

| Message de macOS | Pourquoi | Si on a refusé |
|------------------|----------|----------------|
| « Antenne souhaite contrôler Terminal » | Antenne ouvre une fenêtre du Terminal pour son journal | Réglages Système > Confidentialité et sécurité > **Automatisation** > Antenne > cocher Terminal |
| « Terminal souhaite accéder au micro » | C'est le Terminal qui fait tourner Antenne, et capturer BlackHole compte comme un accès micro | Réglages Système > Confidentialité et sécurité > **Micro** > cocher **Terminal**, puis relancer Antenne |
| « Accepter les connexions entrantes pour antenne-server ? » | Les ESP32 et les autres appareils du réseau se connectent à Antenne | Réglages Système > Réseau > **Coupe-feu** > Options > autoriser `antenne-server` |

> **Micro refusé = silence.** macOS ne signale pas d'erreur : les cartes paraissent en
> service, mais les vu-mètres restent immobiles. C'est la première chose à vérifier quand
> rien ne bouge.

Le coupe-feu peut reposer la question après chaque mise à jour d'Antenne : l'application est
signée « ad hoc », pas avec un certificat Apple.

La configuration est créée au premier lancement dans :

```
~/Library/Application Support/Antenne/antenne.toml
```

## 5. Configurer les cartes

### Le pilote utilisé

Dans `antenne.toml`, section `[platform]`, `loopback_device` doit correspondre au BlackHole
installé :

```toml
[platform]
virtual_backend = "auto"            # sur Mac : découpage des canaux d'un pilote
loopback_device = "BlackHole 16ch"  # ou "BlackHole 2ch", "BlackHole 64ch"
```

Pour ouvrir le fichier dans TextEdit :

```sh
open -e ~/Library/Application\ Support/Antenne/antenne.toml
```

Enregistrer, puis relancer Antenne (bouton **Quitter** de l'interface, puis rouvrir l'app).
Un extrait du nom suffit (`"BlackHole"` trouve aussi le pilote), mais le nom complet évite les
surprises si plusieurs variantes sont installées.

### Les cartes elles-mêmes

Le reste se règle **dans l'interface** : ajouter, renommer, passer en mono ou en stéréo,
changer le codec ([guide de l'interface](06-interface.md)). Les cartes « Carte son
virtuelle » reçoivent leurs canaux BlackHole **dans l'ordre**, en ne comptant que les cartes
virtuelles **en service** :

| Ordre | Carte (exemple) | Canaux | Canaux BlackHole |
|-------|-----------------|--------|------------------|
| 1 | Salon | stéréo | **1-2** |
| 2 | Cuisine | mono | **3** |
| 3 | Atelier | stéréo | **4-5** |
| — | Annonces (tonalité de test) | mono | aucun : ne compte pas |
| 4 | Terrasse | stéréo | **6-7** |

Attention : **supprimer, mettre hors service ou passer de mono à stéréo (ou l'inverse) une
carte virtuelle décale les canaux des cartes qui la suivent**. Antenne recrée alors toutes
les cartes virtuelles ; il faut revoir le routage des applications concernées. Une carte
**ajoutée** se place toujours à la fin et ne décale rien : pour une carte temporaire, mieux
vaut l'ajouter à la fin que réorganiser les autres.

Le journal (fenêtre du Terminal) indique les canaux de chaque carte au démarrage :

```
canaux 1..2 du pilote de bouclage card=salon device=BlackHole 16ch
canaux 3..3 du pilote de bouclage card=cuisine device=BlackHole 16ch
```

### Les autres sources

Une carte peut aussi capturer **une entrée qui existe déjà** (micro, interface audio USB…) :
dans ses réglages, *Source* > *Entrée audio existante*, puis choisir l'entrée dans la liste.
Elle ne consomme alors aucun canal BlackHole. La *Tonalité de test* non plus.

## 6. Router le son vers les cartes

C'est l'étape propre au Mac. Trois cas :

### Une seule carte stéréo

Le plus simple : choisir **BlackHole 16ch** comme sortie, dans l'application ou pour tout le
Mac (Réglages Système > Son > Sortie). Le son part sur les canaux 1-2, donc sur la carte 1.
Le Mac n'émet alors plus de son par ses haut-parleurs (voir « Entendre aussi en local »).

### Plusieurs cartes, avec un logiciel audio qui sait choisir ses canaux

Les logiciels audio (Logic Pro, Ableton Live, Reaper, QLab, Max/MSP…) permettent d'envoyer
chaque piste ou chaque bus **sur des canaux précis** d'une carte son :

1. Choisir **BlackHole 16ch** comme carte de sortie du logiciel.
2. Envoyer chaque piste sur les canaux de sa carte : sorties 1-2 pour « Salon », 3 pour
   « Cuisine », 4-5 pour « Atelier »…

C'est la méthode la plus fiable pour plusieurs zones.

### Plusieurs cartes, avec des applications ordinaires

Une application ordinaire (Musique, Spotify, un navigateur, VLC) envoie toujours sur les
**canaux 1-2** de sa sortie : elle ne peut alimenter que la première carte. Pour diffuser
plusieurs applications ordinaires sur des cartes différentes, il faut un outil de routage
par application, par exemple **Loopback** de Rogue Amoeba (payant) :

- soit Loopback envoie chaque application sur les canaux voulus de BlackHole ;
- soit, plus simple, Loopback crée **une carte virtuelle par pièce** (« Salon », « Cuisine »…)
  et chaque carte d'Antenne utilise la source *Entrée audio existante* correspondante :
  BlackHole n'est alors plus nécessaire.

### Entendre aussi en local

Pour entendre le son sur le Mac en même temps qu'il part vers Antenne : **Configuration audio
et MIDI** > bouton **+** > **Créer un périphérique à sorties multiples**, cocher les
haut-parleurs (ou le casque) **et** BlackHole, puis choisir ce périphérique comme sortie.
Ce montage convient à la première carte (canaux 1-2).

## 7. Vérifier et écouter

1. Ouvrir <http://localhost:8080/> (l'app l'ouvre toute seule au démarrage).
2. Jouer du son : le **vu-mètre** de la carte doit bouger. S'il reste immobile, voir le
   dépannage plus bas.
3. Bouton **Écouter** : le navigateur reçoit la carte en WebRTC, exactement comme un ESP32.

Pour lister les entrées audio que voit Antenne (et vérifier le nom de BlackHole) :

```sh
/Applications/Antenne.app/Contents/MacOS/antenne-server devices
```

## 8. Brancher les ESP32

Les ESP32 se connectent à l'**adresse IP du Mac** sur le réseau local, par exemple
`http://192.168.1.20:8080/whep/salon`. L'adresse IP se trouve dans **Réglages Système >
Wi-Fi > Détails** (ou Ethernet). Il faut que :

- le coupe-feu autorise `antenne-server` (voir les autorisations) ;
- `bind` vaille `0.0.0.0:8080` dans `[server]` (réglage par défaut) ;
- le Mac ne se mette pas en veille pendant la diffusion : Réglages Système > Écran verrouillé /
  Économiseur d'énergie, ou `caffeinate -i` dans un Terminal.

Le protocole côté ESP32 est décrit dans [04-esp32.md](04-esp32.md).

## 9. Arrêter, mettre à jour, désinstaller

**Arrêter** : bouton **Quitter** de l'interface (deux clics), ou **Ctrl+C** dans la fenêtre du
Terminal. Fermer la fenêtre du Terminal arrête aussi Antenne. En dernier recours :

```sh
pkill -x antenne-server
```

**Mettre à jour** : arrêter Antenne, glisser la nouvelle Antenne.app dans Applications
(remplacer). La configuration est conservée. Refaire **clic droit > Ouvrir** au premier
lancement de la nouvelle version.

**Désinstaller** :

```sh
rm -rf /Applications/Antenne.app
rm -rf ~/Library/Application\ Support/Antenne        # configuration
brew uninstall --cask blackhole-16ch                 # si BlackHole n'est plus utile
```

(BlackHole installé sans Homebrew : utiliser le désinstalleur fourni sur le site
d'Existential Audio.)

## 10. Dépannage

| Symptôme | Cause probable | Que faire |
|----------|----------------|-----------|
| « impossible de vérifier le développeur » | Application non notarisée | **Clic droit > Ouvrir**, ou *Ouvrir quand même* dans Confidentialité et sécurité. |
| Rien ne se passe au double-clic | Autorisation « contrôler Terminal » refusée | Confidentialité et sécurité > Automatisation > Antenne > Terminal. |
| « Antenne … tourne déjà sur le port 8080 » | Une Antenne est restée lancée en arrière-plan | Ouvrir <http://localhost:8080/> et cliquer **Quitter**, ou `pkill -x antenne-server`. |
| « le port 8080 est déjà utilisé par un autre programme » | Un autre logiciel utilise le port | Changer `bind` dans `antenne.toml`, par exemple `"0.0.0.0:8090"`. |
| « Address already in use » (Antenne 0.1.2 ou antérieure) | Les deux cas ci-dessus | `pkill -x antenne-server`, puis relancer. |
| « pilote de bouclage introuvable » / « aucune entrée audio ne correspond à BlackHole » | BlackHole absent, ou autre variante installée | Installer BlackHole ; sinon mettre dans `loopback_device` le nom exact listé dans « entrées disponibles ». |
| Cartes en service mais vu-mètres immobiles | Accès micro refusé au Terminal (silence sans erreur), ou application qui ne joue pas vers BlackHole | Confidentialité et sécurité > Micro > Terminal ; vérifier la sortie de l'application. |
| Seule la première carte reçoit du son | L'application envoie sur les canaux 1-2 | Voir « Router le son » : logiciel audio avec choix des canaux, ou Loopback. |
| La carte 3 reçoit le son de la carte 2 | Canaux décalés après une suppression, une mise hors service ou un changement mono / stéréo | Relire les canaux dans le journal et revoir le routage. |
| Dernières cartes en échec (voyant rouge) | Plus de canaux demandés que le pilote n'en a | Passer des cartes en mono, ou installer BlackHole 64ch. |
| L'interface marche sur le Mac mais pas depuis un autre appareil / un ESP32 | Coupe-feu, ou `bind = "127.0.0.1:…"` | Autoriser `antenne-server` dans le coupe-feu ; `bind = "0.0.0.0:8080"`. |
| Coupures au bout d'un moment | Mac en veille | Empêcher la veille pendant la diffusion (`caffeinate -i`). |

## État de la version macOS

La version macOS est compilée et testée automatiquement (CI GitHub, Mac Apple Silicon) et
vient d'être essayée sur un vrai Mac. Les points de ce guide sur les autorisations et le
routage s'appuient sur le fonctionnement standard de macOS et de BlackHole : les retours
d'utilisation sont bienvenus pour les compléter.
