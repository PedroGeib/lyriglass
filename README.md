# Lyriglass

Overlay de vidro leve para o **Spotify** no Windows, com **letras sincronizadas**, efeito karaokê, tradução e controles de reprodução. Ele fica por cima das outras janelas sem atrapalhar.

[![Última versão](https://img.shields.io/github/v/release/PedroGeib/lyriglass)](https://github.com/PedroGeib/lyriglass/releases/latest)
![Plataforma](https://img.shields.io/badge/plataforma-Windows-0078d4)
![Feito com Tauri](https://img.shields.io/badge/feito%20com-Tauri%202-24c8db)
[![Licença MIT](https://img.shields.io/badge/licen%C3%A7a-MIT-green)](LICENSE)

## Download

Baixe o instalador **`Lyriglass_<versão>_x64-setup.exe`** na [página de Releases](https://github.com/PedroGeib/lyriglass/releases/latest) e execute.

O instalador não tem assinatura digital. Na primeira vez, o Windows SmartScreen pode avisar: clique em **Mais informações → Executar assim mesmo**.

## Recursos

- **Letras sincronizadas** do [LRCLIB](https://lrclib.net), com efeito karaokê, ajuste fino de sincronia e "A seguir" com a próxima música.
- **Tradução das letras** pelo Google Tradutor (grátis) ou pelo Gemini (com a sua chave de API), exibida abaixo de cada verso.
- **Cache offline**: músicas já buscadas abrem na hora, mesmo sem internet.
- **Três layouts**: horizontal, vertical e mini.
- **Controles no overlay**: play/pause, volume rolando o mouse sobre a capa e clique em um verso para pular até ele.
- **Visual ajustável**: opacidade, tamanho, cor de destaque tirada da capa, recolher quando não há letra e ocultar quando nada está tocando.
- **Modo Jam**: mostra um QR Code para os amigos entrarem na sua Jam ou abrirem a música atual.
- **Atalhos globais** que funcionam em qualquer lugar do Windows.
- Ícone na bandeja, iniciar com o Windows e posição lembrada entre sessões.

## Primeiros passos: conectar o Spotify

O Lyriglass usa a Web API oficial do Spotify. Para isso, cada pessoa cria um app gratuito no painel de desenvolvedores do Spotify e usa o **Client ID** dele. Leva uns 2 minutos:

1. Abra o **[Spotify Developer Dashboard](https://developer.spotify.com/dashboard)** e entre com a sua conta do Spotify. Na primeira vez, aceite os termos de desenvolvedor.
2. Clique em **Create app**.
3. Em **App name** e **App description**, escreva qualquer coisa (por exemplo, `Lyriglass`).
4. Em **Redirect URIs**, adicione exatamente:
   ```text
   http://127.0.0.1:8888/callback
   ```
5. Em **Which API/SDKs are you planning to use?**, marque **Web API**, aceite os termos e clique em **Save**.
6. Na página do app criado, copie o **Client ID**.
7. No Lyriglass, abra **Configurações → Conta**, cole o Client ID e clique em **Conectar com Spotify**.

O mesmo passo a passo aparece dentro do app, na aba **Conta**.

- Apps novos ficam em **modo de desenvolvimento**: só entram as contas cadastradas em **User Management**, no painel do app. A sua conta, como dona do app, já está liberada.
- **Controlar a reprodução** (play/pause, pular, volume) exige **Spotify Premium**, uma exigência da própria API do Spotify. Ver a música atual e as letras funciona em qualquer conta.
- O login usa **PKCE**: nenhum Client Secret é necessário nem armazenado.

## Atalhos

| Atalho | Ação |
| --- | --- |
| `Ctrl+Alt+H` | Mostrar / ocultar o overlay |
| `Ctrl+Alt+S` | Ativar / desativar click-through |
| `Ctrl+Alt+P` | Play / pause |
| `Ctrl+Alt+L` | Trocar layout |
| `Ctrl+Alt+T` | Mostrar / ocultar tradução |
| `Ctrl+Alt+]` | Adiantar a letra (+250 ms) |
| `Ctrl+Alt+[` | Atrasar a letra (−250 ms) |

No overlay:

| Gesto | Ação |
| --- | --- |
| Rolar o mouse sobre a capa | Volume |
| Clicar em um verso | Pula para aquele trecho |
| Rolar a letra | Navega livremente por 4 s |
| Botão direito | Menu rápido |
| Arrastar a alça do topo | Move o overlay |

## Privacidade

Tudo fica no seu computador, na pasta `%APPDATA%\com.pedrogeib.lyriglass`:

- **Configurações**: `config.json`
- **Tokens do Spotify e chave do Gemini**: `secrets.dat`, criptografado com a proteção de dados do Windows (DPAPI), que só pode ser lido pelo seu usuário do Windows
- **Cache de letras**: `lyrics-cache\`

O app só se comunica com o Spotify, com o LRCLIB e, se a tradução estiver ligada, com o Google Tradutor ou o Gemini.

## Compilar a partir do código

Requisitos:

- Windows 10 ou 11
- [Node.js](https://nodejs.org/) 18 ou mais novo
- [Rust](https://www.rust-lang.org/tools/install)
- [Pré-requisitos do Tauri](https://v2.tauri.app/start/prerequisites/) para Windows (Microsoft C++ Build Tools e WebView2)

```bash
git clone https://github.com/PedroGeib/lyriglass.git
```

```bash
cd lyriglass
```

```bash
npm install
```

Rodar em modo de desenvolvimento:

```bash
npm run dev
```

Gerar o executável e o instalador:

```bash
npm run build
```

O resultado fica em `src-tauri\target\release\` (o `.exe`) e em `src-tauri\target\release\bundle\nsis\` (o instalador).

## Estrutura do projeto

```text
ui/                  Interface (HTML, CSS e JavaScript puro, sem bundler)
  api.js             Ponte com o backend Tauri
  overlay/           Janela do overlay
  settings/          Janela de configurações
src-tauri/           Backend em Rust (Tauri 2)
  src/main.rs        Inicialização e registro dos comandos
  src/shell.rs       Janelas, bandeja, atalhos globais e eventos
  src/commands.rs    Comandos expostos à interface
  src/spotify.rs     Login PKCE e cliente da Web API do Spotify
  src/player.rs      Estado da reprodução e sincronia das letras
  src/lyrics.rs      Busca no LRCLIB, cache e tradução
  src/store.rs       Configurações e segredos criptografados
assets/              Ícone original
```

## Licença

[MIT](LICENSE)

## Aviso

O Lyriglass é um projeto independente e não tem ligação com o Spotify nem é endossado por ele. Spotify é marca registrada da Spotify AB. As letras vêm do LRCLIB e pertencem aos seus respectivos autores.
