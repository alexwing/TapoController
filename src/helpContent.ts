import { Lang } from "./i18n";

export const HELP_MARKDOWN: Record<Lang, string> = {
  es: `# Guía de Conexión y Uso — TapoController

Bienvenido a **TapoController**, una aplicación de escritorio local, rápida y sin dependencia de la nube para controlar bombillas inteligentes **TP-Link Tapo** (modelos L510, L530, L535, L610 y compatibles).

---

## 1. Conexión de las Bombillas (Paso a Paso)

Para que la aplicación pueda comunicarse directamente con tus bombillas a través de tu red Wi-Fi local sin pasar por servidores externos:

### Paso 1: Activar el control local en la app Tapo
1. Abre la app oficial **Tapo** en tu teléfono móvil.
2. Ve a la sección **Yo** (tu perfil) > **Servicios de terceros** (o **Ajustes de cuenta / Control local**).
3. Configura o anota tu **Usuario** (correo de la cuenta TP-Link vinculada) y **Contraseña**.
> *Nota: Esta es la clave de autenticación local que requieren los dispositivos Tapo para aceptar comandos seguros por la red Wi-Fi de tu casa.*

### Paso 2: Introducir credenciales en TapoController
1. En TapoController, haz clic en la pestaña **Ajustes** (⚙️).
2. En la sección **Cuenta TP-Link / Tapo**, introduce tu **Usuario** y **Contraseña**.
3. Pulsa **Guardar y aplicar**.

### Paso 3: Detección automática en red
1. En la misma pantalla de Ajustes, pulsa el botón **Detectar bombillas en la red** (🔍).
2. La aplicación escaneará tu red local en menos de 1 segundo mediante sondeo paralelo.
3. Haz clic en **+ Añadir a la lista** junto a las bombillas encontradas. ¡Listo!

---

## 2. Detección Inteligente por MAC (DHCP Auto-healing)

Muchas veces los routers reinician y asignan una **IP diferente** a las bombillas mediante DHCP, lo que normalmente rompería la conexión.

- **Sin depender de la IP**: TapoController almacena la **dirección MAC** física de cada bombilla.
- **Auto-recuperación**: Si la bombilla cambia de IP, la aplicación detecta el fallo de conexión, busca automáticamente la MAC en tu red local en segundo plano mediante ARP de alta velocidad, actualiza la IP y restablece el control sin que tengas que hacer nada.
- **Búsqueda manual**: Si lo prefieres, en Ajustes puedes pulsar **Buscar por MAC** para forzar la reubicación en cualquier momento.

---

## 3. Modo Multibombilla y Sincronización

TapoController soporta tantas bombillas como tengas en casa:

- **Modo Sincronizado (✦ Todas)**:
  - Selecciona la píldora **✦ Todas** en la parte superior.
  - Cualquier cambio que realices (encender, apagar, variar el brillo, color o efecto arcoíris) se enviará en paralelo a todas las bombillas a la vez.
- **Control Individual**:
  - Selecciona una bombilla específica en la barra superior para ajustarla de forma independiente.
  - En la parte inferior de la pestaña Normal dispones de una cuadrícula de encendido rápido individual para apagar o encender cada bombilla con un solo toque.

---

## 4. Funcionamiento de la Pestaña Normal

- **Botón Hero de Encendido (⏻)**: Pulsa el botón circular grande para alternar el encendido. Se ilumina con brillo verde cuando la luz está encendida.
- **Brillo (1% - 100%)**: Regula la intensidad luminosa con el deslizador principal.
- **Temperatura y Blancos de Máxima Luz**:
  - Deslizador continuo (2500K a 6500K) con gradiente visual de cálido a frío.
  - **Chips directos**:
    - \`2700K Cálido\` ☀️: Luz cálida relajante para la noche.
    - \`4000K Neutro\` 💡: Luz blanca natural para trabajar o leer.
    - \`6500K Frío\` ❄️: Luz blanca fría energizante.
- **Colores y Selector RGB**:
  - 8 muestras circulares de colores vivos predefinidos.
  - Selector circular con degradado que abre la paleta RGB nativa para elegir cualquier tonalidad.
  - Botón **Ajuste fino** (🎚️): Despliega deslizadores de **Tono (0-360°)** y **Saturación (0-100%)** para máxima precisión.
- **Modo Arcoíris (Animado)**:
  - Efecto dinámico que recorre suavemente todo el espectro de color.
  - Incluye selector de velocidad inline.

---

## 5. Modo Ambilight (Streaming de Pantalla)

El modo Ambilight captura en tiempo real los colores que se reproducen en tu monitor y los refleja en tus bombillas, creando una iluminación ambiental inmersiva para películas, series y juegos.

- **Monitor**: Elige qué pantalla capturar en caso de tener varios monitores (identifica el principal con ★).
- **Modos de transición**:
  - **Fundido (suave)**: Interpola suavemente entre los colores para evitar saltos bruscos. Puedes ajustar la suavidad con el control correspondiente.
  - **Instantáneo**: Refleja el cambio de color al milisegundo sin interpolación.
- **FPS y Máx. Hz**:
  - \`FPS\`: Número de capturas de pantalla tomadas por segundo (1 a 30).
  - \`Máx. Hz\`: Tope de envíos a la bombilla por segundo (1 a 20) para evitar saturar la red Wi-Fi.
- **Tope de Brillo**: Limita el brillo máximo durante el streaming para no deslumbrar en habitaciones oscuras.

---

## 6. Modo Disco (Audio del Equipo)

El modo Disco escucha lo que suena por la tarjeta de sonido (música, vídeos, juegos) y hace bailar las bombillas al ritmo. No usa el micrófono: captura la salida de audio directamente (loopback de Windows), así que funciona aunque uses auriculares.

- **Previsualización en vivo**: al abrir la pestaña ves el espectro de lo que suena (graves a la izquierda, agudos a la derecha), los niveles de graves, medios, agudos y volumen, y una **bombilla simulada** que ya aplica el límite de envíos y el suavizado, es decir, lo que hará la bombilla de verdad. Debajo, una línea de tiempo de los últimos 6 s compara el color calculado (arriba) con el de la bombilla (abajo); las marcas blancas son los golpes detectados.
- **Botón «Usar valores recomendados»**: deja el efecto elegido en un punto de partida que funciona; a partir de ahí, ajusta.
- **No envía nada hasta que pulsas ▶**: puedes ajustar el efecto mirando la previsualización y luego empezar a enviarlo. Los cambios se aplican al momento; **Guardar** los deja como predeterminados.
- **Efectos**:
  - **Golpes**: cambia de color en cada golpe de bombo (el *salto de tono* decide cuánto) y destella.
  - **Espectro**: el color sigue a la frecuencia dominante, de rojo (graves) a violeta (agudos).
  - **Pulso**: un color fijo que late con los graves.
  - **Bandas**: graves en rojo, medios en verde y agudos en azul.
- **Ajustes clave**:
  - \`Sensibilidad a golpes\`: súbela si no detecta el ritmo; bájala si destella con todo.
  - \`Ganancia\`: el nivel se normaliza solo con el volumen del sistema; súbela si la música es muy suave.
  - \`Caída del destello\`: cuánto tarda en apagarse cada destello. Valores de 200-400 ms se ven mejor en la bombilla.
  - \`Brillo mínimo\`: la luz que queda entre golpes.
  - \`Máx. actualizaciones/s\` y \`Suavizado\`: la L530 tarda ~200 ms en procesar cada orden (medido), así que el techo real son ~5 cambios por segundo. Deja el suavizado en *Instantáneo* para que los golpes se noten.
- **Animación en la bombilla** (Golpes y Pulso): cuando el tempo lleva 3 s estable, la app programa en la propia bombilla una animación al ritmo del BPM (cambio de color o pulso de brillo cada medio golpe), la arranca en un golpe y la vuelve a alinear cada 16 golpes. Así el pulso no depende del límite de 5 órdenes/s. Si el ritmo se pierde o hay silencio, vuelve al envío normal. La previsualización muestra la animación y la etiqueta *♪ Animación en bombilla* indica cuándo está activa.
- Disco, Ambilight y el arcoíris comparten el mismo canal: al activar uno se detienen los otros.

---

## 7. Servidor API Local (Plugins e Integraciones)

TapoController incluye un servidor HTTP y WebSocket local ultrarrápido (por defecto en \`http://127.0.0.1:7755\`).

- **Casos de uso**: Integración con **Stream Deck**, **Home Assistant**, atajos de teclado, scripts en Python, macros de AutoHotkey o botones personalizados.
- **Endpoints principales**:
  - \`GET /state\`: Estado actual de la bombilla activa.
  - \`GET /bulbs\`: Lista de todas las bombillas configuradas y su estado.
  - \`POST /power\`: \`{"on": true}\` o \`{"on": false}\` (admite opcionalmente \`"bulb_id": "..."\`).
  - \`POST /brightness\`: \`{"brightness": 80}\`.
  - \`POST /color_temp\`: \`{"color_temp": 4000}\`.
  - \`POST /rgb\`: \`{"r": 255, "g": 100, "b": 50}\`.
  - \`POST /sync\`: \`{"sync_all": true}\` para activar o desactivar la sincronización global.
`,

  en: `# Connection & User Guide — TapoController

Welcome to **TapoController**, a fast, lightweight, and cloud-free desktop application to control your **TP-Link Tapo** smart light bulbs (L510, L530, L535, L610, and compatible models).

---

## 1. Connecting Your Bulbs (Step by Step)

To allow the application to communicate directly with your light bulbs over your local Wi-Fi without relying on cloud servers:

### Step 1: Enable local control in the Tapo mobile app
1. Open the official **Tapo** app on your phone.
2. Go to **Me** (profile) > **Third-Party Services** (or **Account Settings / Local Control**).
3. Set or note your **Username** (linked TP-Link account email) and **Password**.
> *Note: This local credential is required by Tapo devices to securely authenticate local commands over your home Wi-Fi.*

### Step 2: Enter credentials in TapoController
1. In TapoController, open the **Settings** tab (⚙️).
2. Under **TP-Link / Tapo Account**, type your **Username** and **Password**.
3. Click **Save & apply**.

### Step 3: Automatic network discovery
1. On the same Settings screen, click **Detect bulbs on network** (🔍).
2. The app scans your local network in under 1 second using parallel probing.
3. Click **+ Add to list** next to the detected bulbs. You are ready!

---

## 2. Intelligent MAC Auto-Detection (DHCP Auto-healing)

Home routers frequently reassign a **different IP address** to bulbs through DHCP after a reboot.

- **IP-Independent**: TapoController saves the unique physical **MAC address** of each bulb.
- **Self-Healing**: If a bulb changes IP, TapoController detects the failure, locates the MAC address on your LAN automatically via ultra-fast ARP, updates the stored IP, and restores control seamlessly.
- **Manual Lookup**: You can also click **Find by MAC** in Settings at any time.

---

## 3. Multi-Bulb Mode & Synchronization

TapoController supports multiple bulbs simultaneously:

- **Synchronized Mode (✦ All)**:
  - Select the **✦ All** pill at the top of the Normal tab.
  - Any change (power toggle, brightness, color, color temperature, or rainbow effect) is broadcast in parallel to all bulbs.
- **Individual Control**:
  - Select any bulb pill in the top bar to adjust it individually.
  - A quick toggle grid is available at the bottom of the Normal tab for single-tap on/off control of each device.

---

## 4. Normal Tab Features

- **Hero Power Button (⏻)**: Single click toggles power. Glows green when the light is on.
- **Brightness (1% - 100%)**: Adjust light output smoothly with the main slider.
- **Temperature & Max White Presets**:
  - Full slider from 2500K to 6500K with a warm-to-cool visual gradient.
  - **Instant chips**:
    - \`2700K Warm\` ☀️: Relaxing warm light for evenings.
    - \`4000K Neutral\` 💡: Balanced natural white for reading and working.
    - \`6500K Cool\` ❄️: Energizing cool daylight.
- **Color Presets & RGB Picker**:
  - 8 vivid preset color dots.
  - Color picker dot with rainbow gradient opening the native RGB color palette.
  - **Fine tune** button (🎚️): Expands **Hue (0-360°)** and **Saturation (0-100%)** sliders for precision.
- **Rainbow Mode (Animated)**:
  - Continuously cycles through the color spectrum with an adjustable speed slider.

---

## 5. Ambilight Mode (Screen Capture)

Ambilight captures the average colors displayed on your screen and projects them onto your bulb in real time, delivering an immersive experience for movies and gaming.

- **Monitor**: Choose which display to capture (marked with ★ for primary display).
- **Transition Modes**:
  - **Fade (smooth)**: Blends smoothly between color changes. Smoothing level can be tuned.
  - **Instant**: Sub-millisecond instant color updates without interpolation.
- **FPS & Max Hz**:
  - \`FPS\`: Screen captures per second (1 to 30).
  - \`Max Hz\`: Upper limit of Wi-Fi bulb updates per second (1 to 20) to prevent network congestion.
- **Brightness Cap**: Restricts maximum brightness so the light remains comfortable in dark rooms.

---

## 6. Disco Mode (PC Audio)

Disco mode listens to whatever your sound card is playing (music, videos, games) and makes the bulbs dance to it. It does not use the microphone: it captures the audio output directly (Windows loopback), so it works with headphones too.

- **Live preview**: the tab shows the spectrum of what is playing (bass on the left, treble on the right), bass/mids/treble/volume meters and a **simulated bulb** that already applies the update-rate limit and smoothing, i.e. what the real bulb will do. Below, a 6-second timeline compares the computed colour (top) with the bulb (bottom); white ticks are detected beats.
- **"Use recommended values" button**: puts the selected effect on a starting point that works; tweak from there.
- **Nothing is sent until you press ▶**: tune the effect while watching the preview, then start sending. Changes apply live; **Save** stores them as defaults.
- **Effects**:
  - **Beats**: changes colour on every kick (the *hue step* sets how far) and flashes.
  - **Spectrum**: colour follows the dominant frequency, from red (bass) to violet (treble).
  - **Pulse**: a fixed colour that throbs with the bass.
  - **Bands**: bass as red, mids as green, treble as blue.
- **Key settings**:
  - \`Beat sensitivity\`: raise it if the rhythm is missed; lower it if everything flashes.
  - \`Gain\`: the level self-normalises to the system volume; raise it for very quiet music.
  - \`Flash release\`: how long each flash takes to fade. 200-400 ms looks best on the bulb.
  - \`Minimum brightness\`: the light left between beats.
  - \`Max updates/s\` and \`Smoothing\`: the L530 needs ~200 ms to process each command (measured), so the real ceiling is ~5 changes per second. Keep smoothing at *Instant* so beats stand out.
- **On-bulb animation** (Beats and Pulse): once the tempo has been stable for 3 s, the app programs an animation on the bulb itself at the BPM (a colour change or brightness pulse every half beat), starts it on a beat and re-aligns it every 16 beats, so the pulse no longer depends on the 5 commands/s limit. If the beat is lost or there is silence, it falls back to normal sending. The preview shows the animation and the *♪ On-bulb animation* tag tells you when it is active.
- Disco, Ambilight and the rainbow share the same channel: starting one stops the others.

---

## 7. Local API Server (Plugins & Integrations)

TapoController includes an ultra-fast local HTTP/WebSocket server (default: \`http://127.0.0.1:7755\`).

- **Integration**: Works with **Stream Deck**, **Home Assistant**, Python scripts, AutoHotkey macros, and custom automations.
- **Key Endpoints**:
  - \`GET /state\`: Current device state.
  - \`GET /bulbs\`: List of all configured bulbs and statuses.
  - \`POST /power\`: \`{"on": true}\` or \`{"on": false}\` (optional: \`"bulb_id": "..."\`).
  - \`POST /brightness\`: \`{"brightness": 80}\`.
  - \`POST /color_temp\`: \`{"color_temp": 4000}\`.
  - \`POST /rgb\`: \`{"r": 255, "g": 100, "b": 50}\`.
  - \`POST /sync\`: \`{"sync_all": true}\` to toggle global sync.
`,

  de: `# Verbindungs- und Benutzerhandbuch — TapoController

Willkommen bei **TapoController**, einer schnellen, cloudfreien Desktop-Anwendung zur lokalen Steuerung von **TP-Link Tapo** Smart-Lampen (L510, L530, L535, L610 und kompatible Modelle).

---

## 1. Lampen verbinden (Schritt für Schritt)

Damit die Anwendung direkt über Ihr lokales WLAN mit den Lampen kommunizieren kann:

### Schritt 1: Lokale Steuerung in der Tapo-App aktivieren
1. Öffnen Sie die offizielle **Tapo**-App auf Ihrem Smartphone.
2. Gehen Sie zu **Ich** (Profil) > **Dienste von Drittanbietern** (oder **Kontoeinstellungen / Lokale Steuerung**).
3. Notieren oder erstellen Sie Ihren **Benutzernamen** (TP-Link E-Mail-Adresse) und Ihr **Passwort**.
> *Hinweis: Diese lokalen Anmeldedaten sind erforderlich, um sichere Befehle im Heimnetzwerk auszuführen.*

### Schritt 2: Zugangsdaten in TapoController eingeben
1. Klicken Sie in TapoController auf den Reiter **Einstellungen** (⚙️).
2. Geben Sie unter **TP-Link / Tapo Konto** Ihren Benutzernamen und Ihr Passwort ein.
3. Klicken Sie auf **Speichern & Anwenden**.

### Schritt 3: Automatische Netzwerkerkennung
1. Klicken Sie in den Einstellungen auf **Lampen im Netzwerk suchen** (🔍).
2. Das Netzwerk wird in unter 1 Sekunde parallel durchsucht.
3. Klicken Sie neben den gefundenen Lampen auf **+ Zur Liste hinzufügen**. Fertig!

---

## 2. Automatische MAC-Auflösung (DHCP Auto-Healing)

Router weisen Lampen bei einem Neustart über DHCP oft eine **neue IP-Adresse** zu.

- **Unabhängig von der IP**: TapoController speichert die physische **MAC-Adresse** jeder Lampe.
- **Automatische Wiederherstellung**: Ändert sich die IP-Adresse, sucht TapoController automatisch im Hintergrund nach der MAC-Adresse, aktualisiert die IP und stellt die Steuerung ohne Benutzereingriff wieder her.
- **Manuelle Suche**: Sie können in den Einstellungen jederzeit auf **Per MAC suchen** klicken.

---

## 3. Mehrlampen-Betrieb & Synchronisation

- **Synchronisierter Modus (✦ Alle)**:
  - Wählen Sie oben die Schaltfläche **✦ Alle**.
  - Jede Aktion (Ein/Aus, Helligkeit, Farbtemperatur, Farbe, Regenbogenmodus) wird gleichzeitig an alle Lampen gesendet.
- **Einzelsteuerung**:
  - Wählen Sie eine Lampe in der oberen Leiste für individuelle Anpassungen.
  - Unten im Reiter Normal befindet sich eine Schnellsteuerung für jede Lampe.

---

## 4. Funktionen im Reiter Normal

- **Hero-Power-Schalter (⏻)**: Schaltet die Lampe direkt ein/aus. Leuchtet grün, wenn die Lampe aktiv ist.
- **Helligkeit (1% - 100%)**: Stufenlose Helligkeitsregelung.
- **Farbtemperatur & Weißtöne**:
  - Schieberegler von 2500K bis 6500K mit optischem Farbverlauf.
  - **Direkttasten**:
    - \`2700K Warmweiß\` ☀️: Gemütliches Licht für den Abend.
    - \`4000K Neutral\` 💡: Ausgewogenes Licht zum Lesen und Arbeiten.
    - \`6500K Kaltweiß\` ❄️: Aktivierendes Tageslicht.
- **Farben & RGB-Wähler**:
  - 8 vordefinierte Farbpunkte.
  - Farbwählerkreis für das native RGB-Spektrum.
  - Schaltfläche **Feinabstimmung** (🎚️): Schieberegler für **Farbton (0-360°)** und **Sättigung (0-100%)**.
- **Regenbogeneffekt**: Animierter kontinuierlicher Farbwechsel mit Geschwindigkeitsregler.

---

## 5. Ambilight-Modus (Bildschirmaufnahme)

Ambilight erfasst die durchschnittliche Farbe Ihres Bildschirms in Echtzeit und überträgt sie auf Ihre Lampen für ein immersives Heimkino- oder Gaming-Erlebnis.

- **Bildschirm**: Auswahl des Monitors (Hauptbildschirm mit ★ markiert).
- **Übergangsmodi**:
  - **Überblenden (weich)**: Sanfter Farbwechsel mit einstellbarer Glättung.
  - **Sofort**: Extrem schnelle Direktübertragung ohne Übergang.
- **FPS & Max. Hz**:
  - \`FPS\`: Bildschirmaufnahmen pro Sekunde (1 bis 30).
  - \`Max. Hz\`: Maximale Netzwerkbefehle pro Sekunde (1 bis 20).
- **Helligkeitsobergrenze**: Begrenzung der Maximalhelligkeit für dunkle Räume.

---

## 6. Disco-Modus (PC-Audio)

Der Disco-Modus hört, was die Soundkarte abspielt (Musik, Videos, Spiele), und lässt die Lampen im Takt tanzen. Er nutzt kein Mikrofon, sondern nimmt den Audioausgang direkt ab (Windows-Loopback) und funktioniert daher auch mit Kopfhörern.

- **Live-Vorschau**: Spektrum (Bass links, Höhen rechts), Pegel für Bass/Mitten/Höhen/Lautstärke und eine **simulierte Lampe**, die Update-Limit und Glättung schon berücksichtigt. Darunter vergleicht eine 6-Sekunden-Zeitleiste die berechnete Farbe (oben) mit der Lampe (unten); weiße Striche sind erkannte Beats.
- **„Empfohlene Werte“**: setzt den gewählten Effekt auf einen funktionierenden Ausgangspunkt.
- **Gesendet wird erst nach ▶**: Effekt in der Vorschau einstellen, dann starten. Änderungen wirken sofort; **Speichern** macht sie zum Standard.
- **Effekte**: **Beats** (Farbwechsel und Blitz bei jedem Bassschlag), **Spektrum** (Farbe folgt der dominanten Frequenz), **Puls** (feste Farbe pulsiert mit dem Bass), **Bänder** (Bass rot, Mitten grün, Höhen blau).
- **Tipps**: Empfindlichkeit erhöhen, wenn Beats fehlen; Abklingzeit 200-400 ms; die L530 braucht ~200 ms pro Befehl (gemessen), also maximal ~5 Änderungen pro Sekunde.
- **Animation in der Lampe** (Beats und Puls): Bei 3 s stabilem Tempo programmiert die App eine Animation im BPM-Takt direkt in der Lampe, startet sie auf einem Beat und richtet sie alle 16 Beats neu aus – ohne 5/s-Limit. Ohne Beat wird wieder normal gesendet.
- Disco, Ambilight und Regenbogen teilen sich denselben Kanal: Startet einer, stoppen die anderen.

---

## 7. Lokale API (Plugins & Automatisierung)

Integrierter lokaler HTTP- und WebSocket-Server (\`http://127.0.0.1:7755\`).

- **Integration**: Ideal für **Stream Deck**, **Home Assistant**, Python-Skripte oder AutoHotkey-Makros.
- **Wichtige Endpunkte**: \`/state\`, \`/bulbs\`, \`/power\`, \`/brightness\`, \`/color_temp\`, \`/rgb\`, \`/sync\`.
`,

  fr: `# Guide de Connexion et d'Utilisation — TapoController

Bienvenue sur **TapoController**, une application de bureau locale, rapide et sans cloud pour contrôler vos ampoules connectées **TP-Link Tapo** (L510, L530, L535, L610 et modèles compatibles).

---

## 1. Connexion des Ampoules (Étape par étape)

Pour permettre à l'application de communiquer directement avec vos ampoules via votre réseau Wi-Fi local :

### Étape 1 : Activer le contrôle local dans l'application Tapo
1. Ouvrez l'application officielle **Tapo** sur votre smartphone.
2. Allez dans **Moi** (profil) > **Services tiers** (ou **Paramètres du compte / Contrôle local**).
3. Notez ou configurez votre **Nom d'utilisateur** (e-mail TP-Link) et votre **Mot de passe**.
> *Remarque : Cet identifiant local est obligatoire pour sécuriser les commandes sur votre réseau Wi-Fi.*

### Étape 2 : Saisir les identifiants dans TapoController
1. Ouvrez l'onglet **Paramètres** (⚙️) dans TapoController.
2. Sous **Compte TP-Link / Tapo**, entrez votre Nom d'utilisateur et votre Mot de passe.
3. Cliquez sur **Enregistrer & appliquer**.

### Étape 3 : Détection automatique sur le réseau
1. Dans les Paramètres, cliquez sur **Détecter les ampoules sur le réseau** (🔍).
2. L'application scanne votre réseau en moins d'une seconde.
3. Cliquez sur **+ Ajouter à la liste** pour chaque ampoule trouvée. C'est tout !

---

## 2. Détection intelligente par adresse MAC (DHCP Auto-healing)

Les box internet et routeurs réassignent souvent une **nouvelle adresse IP** aux ampoules via DHCP.

- **Indépendant de l'IP** : TapoController enregistre l'adresse **MAC** unique de chaque appareil.
- **Récupération automatique** : Si l'IP change, TapoController détecte la coupure, localise la MAC sur le réseau local via ARP, met à jour l'IP et rétablit la communication en arrière-plan.
- **Recherche manuelle** : Vous pouvez cliquer à tout moment sur **Chercher par MAC** dans les Paramètres.

---

## 3. Mode Multi-ampoules et Synchronisation

- **Mode Synchronisé (✦ Toutes)** :
  - Sélectionnez le bouton **✦ Toutes** en haut de l'onglet Normal.
  - Tout changement (alimentation, luminosité, température, couleur, arc-en-ciel) est transmis simultanément à toutes vos ampoules.
- **Contrôle Individuel** :
  - Cliquez sur une ampoule précise en haut pour l'ajuster individuellement.
  - Une grille de contrôle rapide en bas permet d'allumer ou d'éteindre chaque ampoule d'un simple clic.

---

## 4. Fonctionnalités de l'Onglet Normal

- **Bouton d'alimentation Hero (⏻)** : Allume ou éteint d'un clic. Brille en vert lorsque l'ampoule est allumée.
- **Luminosité (1% - 100%)** : Curseur fluide d'intensité lumineuse.
- **Température & Blancs purs** :
  - Curseur de 2500K à 6500K avec dégradé visuel du chaud au froid.
  - **Boutons rapides** :
    - \`2700K Chaud\` ☀️ : Blanc chaud relaxant pour le soir.
    - \`4000K Neutre\` 💡 : Blanc équilibré pour travailler et lire.
    - \`6500K Froid\` ❄️ : Lumière du jour stimulante.
- **Couleurs & Sélecteur RGB** :
  - 8 pastilles de couleurs vives.
  - Pastille dégradée ouvrant la palette RGB native.
  - Bouton **Réglage fin** (🎚️) : Affiche les curseurs de **Teinte (0-360°)** et **Saturation (0-100%)**.
- **Mode Arc-en-ciel (Animé)** : Cycle dynamique de couleurs avec réglage de la vitesse.

---

## 5. Mode Ambilight (Capture d'Écran)

Ambilight analyse en direct la couleur moyenne de votre écran pour éclairer votre pièce en harmonie avec vos films, séries et jeux vidéo.

- **Écran** : Choisissez l'écran à capturer (l'écran principal est noté ★).
- **Modes de transition** :
  - **Fondu (doux)** : Changement progressif et fluide avec intensité réglable.
  - **Instantané** : Réponse immédiate sans transition.
- **FPS & Max Hz** :
  - \`FPS\` : Captures d'écran par seconde (1 à 30).
  - \`Max Hz\` : Envois maximum par seconde (1 à 20) pour préserver la bande passante.
- **Luminosité max** : Limiteur de luminosité pour le confort dans l'obscurité.

---

## 6. Mode Disco (audio du PC)

Le mode Disco écoute ce que joue la carte son (musique, vidéos, jeux) et fait danser les ampoules au rythme. Il n'utilise pas le micro : il capture directement la sortie audio (loopback Windows), donc il fonctionne aussi au casque.

- **Aperçu en direct** : spectre (graves à gauche, aigus à droite), niveaux graves/médiums/aigus/volume et une **ampoule simulée** qui applique déjà la limite d'envois et le lissage. En dessous, une frise de 6 s compare la couleur calculée (haut) et l'ampoule (bas) ; les traits blancs sont les battements détectés.
- **« Valeurs recommandées »** : met l'effet choisi sur un point de départ qui fonctionne.
- **Rien n'est envoyé avant ▶** : réglez l'effet en regardant l'aperçu, puis lancez l'envoi. Les changements s'appliquent en direct ; **Enregistrer** les garde par défaut.
- **Effets** : **Battements** (change de couleur et clignote à chaque grosse caisse), **Spectre** (la couleur suit la fréquence dominante), **Pulsation** (couleur fixe qui bat avec les graves), **Bandes** (graves rouge, médiums vert, aigus bleu).
- **Conseils** : montez la sensibilité si le rythme n'est pas détecté ; décroissance de 200-400 ms ; la L530 met ~200 ms par commande (mesuré), soit ~5 changements par seconde au maximum.
- **Animation dans l'ampoule** (Battements et Pulsation) : après 3 s de tempo stable, l'application programme dans l'ampoule une animation au BPM, la lance sur un battement et la réaligne tous les 16 battements, sans la limite de 5/s. Sans rythme, retour à l'envoi normal.
- Disco, Ambilight et l'arc-en-ciel partagent le même canal : en lancer un arrête les autres.

---

## 7. Serveur API Local (Plugins et Intégrations)

Serveur HTTP/WebSocket local (\`http://127.0.0.1:7755\`).

- **Compatibilité** : Idéal pour **Stream Deck**, **Home Assistant**, scripts Python et raccourcis clavier.
- **Points d'accès clés** : \`/state\`, \`/bulbs\`, \`/power\`, \`/brightness\`, \`/color_temp\`, \`/rgb\`, \`/sync\`.
`,

  pt: `# Guia de Ligação e Utilização — TapoController

Bem-vindo ao **TapoController**, uma aplicação de ambiente de trabalho rápida, local e sem dependência da nuvem para controlar lâmpadas inteligentes **TP-Link Tapo** (modelos L510, L530, L535, L610 e compatíveis).

---

## 1. Como Ligar as Lâmpadas (Passo a Passo)

Para permitir que a aplicação comunique diretamente com as suas lâmpadas através da sua rede Wi-Fi local:

### Passo 1: Ativar o controlo local na aplicação Tapo
1. Abra a aplicação oficial **Tapo** no seu telemóvel.
2. Aceda a **Eu** (perfil) > **Serviços de terceiros** (ou **Definições de conta / Controlo local**).
3. Configure ou anote o seu **Utilizador** (e-mail da conta TP-Link associada) e a sua **Palavra-passe**.
> *Nota: Esta credencial local é necessária para que os dispositivos Tapo aceitem comandos seguros na rede Wi-Fi doméstica.*

### Passo 2: Introduzir credenciais no TapoController
1. No TapoController, clique no separador **Definições** (⚙️).
2. Na secção **Conta TP-Link / Tapo**, introduza o seu Utilizador e Palavra-passe.
3. Clique em **Guardar e aplicar**.

### Passo 3: Deteção automática na rede
1. Nas Definições, clique no botão **Detetar lâmpadas na rede** (🔍).
2. A aplicação fará a pesquisa em menos de 1 segundo.
3. Clique em **+ Adicionar à lista** junto das lâmpadas encontradas. Concluído!

---

## 2. Resolução Inteligente por MAC (DHCP Auto-healing)

Frequentemente, os routers alteram o **endereço IP** dos dispositivos através do DHCP após um reinício.

- **Independente do IP**: O TapoController guarda o endereço físico **MAC** de cada lâmpada.
- **Recuperação Automática**: Se o IP mudar, a aplicação deteta a falha, localiza o MAC na rede local via ARP de alta velocidade, atualiza o IP e restabelece a ligação automaticamente.
- **Pesquisa Manual**: Pode clicar em **Procurar por MAC** nas Definições a qualquer momento.

---

## 3. Modo Multi-Lâmpada e Sincronização

- **Modo Sincronizado (✦ Todas)**:
  - Selecione o botão **✦ Todas** no topo do separador Normal.
  - Qualquer alteração (ligar, desligar, brilho, cor, efeito arco-íris) é enviada em simultâneo para todas as lâmpadas.
- **Controlo Individual**:
  - Selecione uma lâmpada no topo para ajustes individuais.
  - Na parte inferior do separador Normal, encontra uma grelha rápida para ligar/desligar cada lâmpada individualmente.

---

## 4. Funcionalidades do Separador Normal

- **Botão Hero de Ligar/Desligar (⏻)**: Alterna o estado da luz com um clique. Ilumina-se a verde quando a lâmpada está ligada.
- **Brilho (1% - 100%)**: Controlo deslizante suave de intensidade.
- **Temperatura e Brancos Puros**:
  - Deslizador de 2500K a 6500K com gradiente visual de quente a frio.
  - **Botões Rápidos**:
    - \`2700K Quente\` ☀️: Luz quente e acolhedora para o descanso.
    - \`4000K Neutro\` 💡: Luz natural equilibrada para leitura e trabalho.
    - \`6500K Frio\` ❄️: Luz branca fria estimulante.
- **Cores & Seletor RGB**:
  - 8 amostras de cores vivas pré-definidas.
  - Círculo de gradiente que abre o seletor de cor RGB nativo.
  - Botão **Ajuste fino** (🎚️): Revela os controlos de **Matiz (0-360°)** e **Saturação (0-100%)**.
- **Modo Arco-íris (Animado)**: Ciclo contínuo de cores com ajuste de velocidade.

---

## 5. Modo Ambilight (Captura de Ecrã)

O modo Ambilight analisa as cores médias apresentadas no monitor e reflete-as nas lâmpadas em tempo real para uma experiência imersiva em filmes e jogos.

- **Monitor**: Escolha o ecrã a capturar (o principal é assinalado com ★).
- **Modos de Transição**:
  - **Desvanecer (suave)**: Transição fluida com nível de suavidade regulável.
  - **Instantâneo**: Resposta imediata sem interpolação.
- **FPS & Máx. Hz**:
  - \`FPS\`: Capturas de ecrã por segundo (1 a 30).
  - \`Máx. Hz\`: Limite de comandos por segundo para a lâmpada (1 a 20).
- **Brilho Máximo**: Limita o brilho para ambientes escuros.

---

## 6. Modo Disco (áudio do PC)

O modo Disco ouve o que a placa de som está a reproduzir (música, vídeos, jogos) e faz as lâmpadas dançar ao ritmo. Não usa o microfone: captura diretamente a saída de áudio (loopback do Windows), por isso também funciona com auscultadores.

- **Pré-visualização ao vivo**: espectro (graves à esquerda, agudos à direita), níveis de graves/médios/agudos/volume e uma **lâmpada simulada** que já aplica o limite de envios e a suavização. Por baixo, uma linha temporal de 6 s compara a cor calculada (cima) com a lâmpada (baixo); as marcas brancas são batidas detetadas.
- **«Usar valores recomendados»**: põe o efeito escolhido num ponto de partida que funciona.
- **Nada é enviado antes de ▶**: ajuste o efeito a ver a pré-visualização e depois comece a enviar. As alterações aplicam-se de imediato; **Guardar** fixa-as como predefinição.
- **Efeitos**: **Batidas** (muda de cor e pisca a cada bumbo), **Espectro** (a cor segue a frequência dominante), **Pulso** (cor fixa que pulsa com os graves), **Bandas** (graves vermelho, médios verde, agudos azul).
- **Dicas**: suba a sensibilidade se o ritmo não for detetado; queda de 200-400 ms; a L530 demora ~200 ms por ordem (medido), ou seja ~5 mudanças por segundo no máximo.
- **Animação na lâmpada** (Batidas e Pulso): com 3 s de tempo estável, a aplicação programa na própria lâmpada uma animação ao BPM, arranca-a numa batida e realinha-a a cada 16 batidas, sem o limite de 5/s. Sem ritmo, volta ao envio normal.
- Disco, Ambilight e o arco-íris partilham o mesmo canal: ligar um desliga os outros.

---

## 7. Servidor API Local (Plugins e Integrações)

Servidor HTTP e WebSocket local (\`http://127.0.0.1:7755\`).

- **Casos de Utilização**: Integração com **Stream Deck**, **Home Assistant**, scripts em Python ou atalhos de teclado.
- **Endpoints Principais**: \`/state\`, \`/bulbs\`, \`/power\`, \`/brightness\`, \`/color_temp\`, \`/rgb\`, \`/sync\`.
`,
};
