# Como o bt-volume funciona

Este documento explica o funcionamento interno do programa: a arquitetura, o
caminho que um evento de áudio percorre, por que existem várias tentativas de
restauração e como o estado é persistido.

Para instruções de uso e instalação, veja o [`README.md`](README.md).

---

## 1. Visão geral

O objetivo é simples: cada fone Bluetooth tem um volume preferido pelo usuário.
O Windows, porém, esquece (ou redefine) esse volume a cada nova conexão, porque
muitos fones enviam o próprio volume absoluto via AVRCP logo depois de parear.

O programa faz quatro coisas:

1. **Observa** quais endpoints de áudio Bluetooth estão ativos.
2. **Lembra** o volume de cada um em um arquivo texto.
3. **Restaura** esse volume quando o mesmo dispositivo volta a conectar.
4. **Ignora** as mudanças de volume que ele mesmo provocou.

Não há serviço, driver, instalador, banco de dados nem runtime adicional. É um
único `.exe` de ~4,7 MB que fica na bandeja do sistema. O tamanho vem do egui,
usado apenas pela janela opcional de dispositivos.

---

## 2. Arquitetura

O processo tem **três threads**:

- **principal** — fica parada em `ui::run_loop()`, esperando um pedido da
  bandeja. Quando o usuário abre a janela, ela executa o egui (que exige a
  *main thread* no Windows) e volta a dormir quando a janela fecha.
- **tray** — janela oculta do Win32, ícone, menu, tooltip e balões.
- **audio-engine** — todo o COM/Core Audio.

```
┌─────────────────────────────────────────────────────────────┐
│ Thread principal                                            │
│                                                             │
│  main()                                                     │
│    ├─ cria %LOCALAPPDATA%\bt-volume                         │
│    ├─ abre o log                                            │
│    ├─ adquire o mutex de instância única                    │
│    ├─ carrega devices.tsv para a memória                    │
│    ├─ sobe a thread do motor de áudio ────────────┐         │
│    ├─ sobe a thread da bandeja ────────────┐      │         │
│    └─ ui::run_loop()                       │      │         │
│         ├─ espera pedido (canal mpsc)      │      │         │
│         ├─ ao abrir: eframe::run_native    │      │         │
│         └─ ao fechar: volta a esperar      │      │         │
└────────────────────────────────────────────┼──────┼─────────┘
                                             │      │
                Show / Exit (ui::request_*)  │      │ AudioRequest::
                                             ▼      ▼ Shutdown,
┌──────────────────────────────────┐   ┌────────────────────────┐
│ Thread "tray"                    │   │ Thread "audio-engine"  │
│                                  │   │ (COM MTA)              │
│  GetMessage / DispatchMessage    │   │                        │
│    • Shell_NotifyIconW           │   │  IMMDeviceEnumerator   │
│    • menu de contexto            │   │  callbacks de endpoint │
│    • tooltip e balões            │   │  loop de 200 ms        │
└──────────────────────────────────┘   └────────────────────────┘
```

Regras dessa divisão:

- **Todo acesso a COM acontece na thread do motor**, sempre na mesma thread MTA.
  Isso evita problemas de apartamento e de reentrância.
- Os *callbacks* de COM (`IMMNotificationClient`,
  `IAudioEndpointVolumeCallback`) **não fazem trabalho**: apenas enviam uma
  mensagem pequena (`Event`) por um canal `mpsc`. O motor processa o evento
  depois, no seu próprio loop.
- A thread da bandeja conversa com o motor por outro canal `mpsc`
  (`AudioRequest`). O motor responde indiretamente, publicando o estado em
  `Shared::devices` (protegido por `Mutex`) e empilhando textos de notificação em
  `Shared::notes`.
- O store é compartilhado por `Arc<Shared>` com um `Mutex<Store>`; as três
  threads podem ler o caminho do arquivo e enfileirar notificações, mas só o
  motor escreve níveis.
- A janela do egui **não é criada no boot**. Ela nasce quando o usuário clica no
  ícone, e é destruída quando ele a fecha. Isso mantém o caminho “só bandeja”
  sem contexto OpenGL e evita a janela piscando na inicialização — o eframe
  força a janela a ficar visível depois do primeiro frame, então escondê-la não
  funcionaria.
- A bandeja não chama o egui diretamente: envia pedidos (`Show`, `Exit`) pelo
  canal e usa `ui::is_available()` para saber se a janela pode ser aberta.

### Mapa dos arquivos

| Arquivo | Papel |
| --- | --- |
| `src/main.rs` | entrada mínima da aplicação |
| `src/app.rs` | bootstrap, registro de comandos, criação das threads, encerramento |
| `src/core/command.rs` | padrão Command: trait, contexto e registro |
| `src/core/state.rs` | `Shared`, `DeviceStatus`, `SavedDevice` |
| `src/core/util/` | caminhos, log, strings wide, diálogos, instância única |
| `src/commands/` | um arquivo por funcionalidade/comando |
| `src/features/audio/` | motor Core Audio: `engine`, `endpoint`, `notifier`, `policy`, `request`, `com` |
| `src/features/store/` | store persistente em TSV e conversões de escala |
| `src/features/tray/` | janela oculta, ícone, menu, tooltip, balões |
| `src/features/ui/` | janela de dispositivos em egui e fontes de fallback |
| `src/features/autostart/` | chave `HKCU\...\Run` |

### 2.1 O padrão Command

Toda funcionalidade visível — mostrar status, abrir a janela, restaurar volumes,
abrir a pasta de dados ou o log, alternar o autostart e sair — é um objeto que
implementa `core::command::Command`:

```rust
pub trait Command: Send + Sync {
    fn id(&self) -> CommandId;
    fn label(&self) -> &'static str;
    fn checked(&self) -> bool { false }
    fn run(&self, ctx: &CommandContext) -> CommandOutcome;
}
```

`CommandContext` entrega ao comando o `Arc<Shared>`, o canal para o motor
(`Sender<AudioRequest>`) e um `Notifier` opcional para balões da bandeja. Assim o
mesmo comando funciona tanto no menu quanto nos botões da janela. A bandeja monta
o menu a partir dos ids registrados em `commands::standard()`, e o registro é
criado uma única vez em `app.rs` e compartilhado por `Arc` com a bandeja e a UI.
Para adicionar uma funcionalidade, basta criar um arquivo em `src/commands/`,
registrá-lo no `standard()` e, se necessário, incluir o id no layout do menu.

---

## 3. Inicialização

`main()` executa, nesta ordem:

1. Cria `%LOCALAPPDATA%\bt-volume` (`util::ensure_data_dir`). Se falhar, mostra
   uma caixa de erro e sai com código de falha — sem pasta de dados não há como
   lembrar nada.
2. Abre o log em modo *append* e rotaciona antes, se o arquivo passar de 1 MB
   (renomeia para `bt-volume.log.1`).
3. Adquire o mutex nomeado `Local\bt-volume-single-instance`. Se já existir,
   avisa que o programa está rodando e encerra com sucesso. `Local\` significa
   “por sessão de logon”, então cada usuário pode ter a sua instância.
4. Carrega `devices.tsv` para um `BTreeMap` em memória. Um arquivo ausente ou
   linhas malformadas não são erro: simplesmente resultam em store vazio.
5. Sobe a thread `audio-engine`.
6. Sobe a thread `tray`, que cria uma janela **invisível** (`WS_OVERLAPPED`,
   0×0) apenas para receber mensagens, registra o ícone e roda o loop
   `GetMessage`/`DispatchMessage`.
7. Chama `ui::init()`, que cria o canal de pedidos da janela, e entra em
   `ui::run_loop()`. Com `--show` na linha de comando, um pedido de abertura já
   entra na fila.

No encerramento, o menu **Exit** destrói a janela da bandeja; o loop termina, o
ícone é removido e a thread chama `ui::request_exit()`. O `run_loop` então
retorna (fechando a janela do egui, se estiver aberta), `main` envia
`AudioRequest::Shutdown`, faz `join` na bandeja e no motor, e sai. O motor, ao sair,
desregistra callbacks, faz o *flush* final do store e limpa a lista de
dispositivos exibida.

---

## 4. Descoberta dos dispositivos

### 4.1 Enumeração

`enumerate_endpoints()` chama:

```rust
enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
```

Ou seja: apenas **endpoints de reprodução ativos**. Cada item passa por
`endpoint_from_device()`, que:

1. Lê o **id do endpoint** (`IMMDevice::GetId`), liberando a string com
   `CoTaskMemFree`. Esse id tem o formato
   `{0.0.0.00000000}.{058eb3bd-...}` e é a chave primária do store.
2. Abre o `IPropertyStore` do dispositivo e lê três propriedades:

   | Constante | Property key | Uso |
   | --- | --- | --- |
   | `PKEY_DEVICE_FRIENDLY_NAME` | `a45c254e-df1c-4efd-8020-67d146a850e0`, pid 14 | nome exibido e fallback de identificação |
   | `PKEY_DEVICE_ENUMERATOR_NAME` | mesma `fmtid`, pid 24 | detecção de Bluetooth |
   | `PKEY_DEVICE_INSTANCE_ID` | `78c34fc8-104a-4aca-9ea4-524d52996e57`, pid 256 | detecção de Bluetooth |

   Elas são declaradas localmente em `audio.rs` para não precisar de features
   extras da crate `windows`.
3. Decide se é Bluetooth:

   ```rust
   instance.to_ascii_uppercase().starts_with("BTH")
       || enumerator_name.to_ascii_uppercase().starts_with("BTH")
   ```

   O prefixo `BTH` cobre A2DP, mãos-livres e LE Audio. O teste é feito nos dois
   campos porque **alguns fones retornam `InstanceId` vazio** — foi exatamente o
   caso do aparelho usado nos testes. Se apenas o `InstanceId` fosse checado, o
   dispositivo nunca seria reconhecido.
4. Ativa `IAudioEndpointVolume` no endpoint e lê
   `GetMasterVolumeLevelScalar()`, um `f32` de `0.0` a `1.0`.
5. Devolve um `Endpoint { id, name, level, volume }`.

Dispositivos com falha de leitura são registrados no log e ignorados, sem
interromper a enumeração dos demais.

### 4.2 Por que existe polling

`IMMNotificationClient` entrega `OnDeviceAdded`, `OnDeviceRemoved`,
`OnDeviceStateChanged` e `OnDefaultDeviceChanged`. Na prática, alguns drivers
Bluetooth não emitem (ou atrasam) esses avisos quando o fone é ligado.

Por isso o loop do motor também refaz o scan completo a cada
`SCAN_INTERVAL = 3 s`. As duas fontes são complementares:

- **Callback** → reação imediata quando o Windows avisa.
- **Polling** → rede de segurança quando o aviso não vem.

`OnPropertyValueChanged` é ignorado de propósito: ele dispara com muita
frequência e não afeta o que o programa faz.

---

## 5. Conexão de um dispositivo

`scan()` compara o conjunto de endpoints ativos com o que já está em
`self.tracked` (um `HashMap<id, Tracked>`):

- **Já rastreado** → apenas atualiza `level` e `name` com os valores atuais.
- **Novo** → `add_device()`.
- **Sumiu da lista** → `drop_device()`.

Em `add_device()`:

1. Cria um `VolumeNotifier` para aquele endpoint e chama
   `RegisterControlChangeNotify`. A partir daí, qualquer mudança de volume
   daquele endpoint gera um `Event::Volume` no canal.
2. Procura um nível salvo com `stored_level(id, name)`.
3. **Se já existe nível salvo** → agenda uma restauração (`PendingRestore`) para
   o instante atual e mostra “*conectado · restaurando volume para N%*”.
4. **Se é a primeira vez** → salva o volume atual imediatamente, grava o store em
   disco e mostra “*conectado · volume atual salvo (N%)*”.

O dispositivo entra em `Tracked` com `connected_at = now` e `changed_at = None`.
Esse `connected_at` é o relógio usado pela janela de carência descrita adiante.

---

## 6. Restauração do volume

### 6.1 Máquina de estados simples

Uma restauração pendente é:

```rust
struct PendingRestore {
    id: String,     // endpoint alvo
    units: u32,     // nível salvo (0..10000), só para log/porcentagem
    target: f32,    // nível salvo como escalar (0.0..1.0)
    attempts: u32,  // quantas vezes já aplicamos
    next: Instant,  // quando aplicar a próxima tentativa
}
```

O loop do motor chama `apply_due_restores(now)` a cada iteração. Para cada
pendência vencida, ele executa:

```rust
tracked.volume.SetMasterVolumeLevelScalar(pending.target, &BT_VOLUME_CONTEXT)
```

### 6.2 Linha do tempo de uma conexão

| Momento | Ação |
| --- | --- |
| `t = 0` | endpoint detectado; tentativa 1 aplicada imediatamente |
| `t = 0,6 s` | tentativa 2 (+600 ms) |
| `t = 2,0 s` | tentativa 3 (+1400 ms) |
| `t = 4,8 s` | tentativa 4, última (+2800 ms) |
| `t ≥ 3,0 s` | mudanças de volume passam a ser salvas normalmente |

Os valores de `RETRY_DELAYS` são somados ao instante da tentativa anterior, não
ao instante da conexão:

```rust
pending.next = now + RETRY_DELAYS[(pending.attempts - 1) as usize];
```

Por isso a sequência real é **0 / 0,6 / 2,0 / 4,8 s**. Como a carência de
salvamento termina em 3 s, a última tentativa acontece depois dela — mas isso
não corrompe o store, porque a escrita carrega o `BT_VOLUME_CONTEXT` e é
descartada em `on_volume`. O efeito prático é que uma mudança feita pelo usuário
entre 3 s e 4,8 s pode ser sobrescrita pela última tentativa.

`MAX_ATTEMPTS = 4`; depois da última, o programa lê o volume real com
`GetMasterVolumeLevelScalar` e, se a diferença em relação ao alvo for maior que
1%, escreve um aviso no log. Isso acontece quando o fone impõe o próprio volume
depois da última tentativa — é o limite do que dá para fazer sem um driver.

### 6.3 Por que aplicar quatro vezes

Muitos headsets fazem, ao conectar:

```
Windows ativa o endpoint
      ↓
fone envia seu volume absoluto via AVRCP
      ↓
Windows aplica esse volume, sobrescrevendo o que definimos
```

A ordem e o atraso desse envio variam por modelo. Uma única aplicação
funcionaria em alguns fones e falharia em outros. As quatro tentativas, com
intervalos crescentes, cobrem a janela em que o fone costuma “brigar” pelo
volume, sem gerar escritas contínuas no dispositivo.

### 6.4 Como o programa reconhece a própria escrita

`SetMasterVolumeLevelScalar` aceita um `GUID` de contexto de evento. O programa
usa sempre o mesmo GUID privado:

```rust
const BT_VOLUME_CONTEXT: GUID =
    GUID::from_u128(0x5b17e4b2_1f8c_4a6d_9a3e_7c2d4f6b8a01);
```

Esse GUID chega de volta no callback em `AUDIO_VOLUME_NOTIFICATION_DATA::guidEventContext`.
`on_volume()` compara e **descarta** o evento se for igual. Sem isso, cada
restauração seria interpretada como “o usuário mudou o volume” e o nível salvo
seria reescrito com o valor que acabamos de aplicar — inofensivo na maioria das
vezes, mas capaz de consolidar um valor errado se o fone alterasse o volume
entre as tentativas.

### 6.5 Restauração manual

O item **Restore saved volumes** do menu executa o comando `RestoreAll`, que
envia `AudioRequest::RestoreAll` ao motor. O motor
percorre os dispositivos rastreados, busca o nível salvo de cada um e agenda uma
restauração com `next = now` — mesma máquina de estados, mesmos quatro toques.
Se nenhum tiver nível salvo, mostra um balão informando isso.

---

## 7. Salvamento do volume

`on_volume(id, level, context)` é o ponto de entrada para mudanças reais:

1. Descarta o evento se `context == BT_VOLUME_CONTEXT` (escrita nossa).
2. Descarta se o endpoint não está sendo rastreado (fone desconectado no meio).
3. Atualiza `tracked.level` e `tracked.changed_at = now`.
4. **Carência de conexão**: se `now - connected_at < 3 s`, retorna sem salvar.
5. Caso contrário, converte para `units` e grava no store em memória.

### 7.1 A janela de carência de 3 segundos

Os primeiros segundos após a conexão são dominados pelo sincronismo de volume do
próprio fone. Se o programa salvasse tudo nesse intervalo, o nível memorizado
seria justamente o valor que o fone impôs — e não a preferência do usuário. Por
isso `CONNECT_GRACE = 3 s`: mudanças nesse intervalo são registradas em
`changed_at` (útil na desconexão, veja abaixo), mas não sobrescrevem o store.

Efeito colateral documentado: mudanças feitas entre 3 s e o fim da última
tentativa (4,8 s) são salvas, mas podem ser sobrescritas pela restauração que
ainda está pendente. Depois da última tentativa, tudo é salvo normalmente.

### 7.2 Desconexão

`drop_device()` faz o desregistro do callback e, antes de esquecer o
dispositivo, decide se ainda deve salvar:

```rust
if let Some(changed) = tracked.changed_at {
    if changed.duration_since(tracked.connected_at) >= CONNECT_GRACE {
        store.set(id, name, units);
        store.flush();
    }
}
```

Ou seja: só grava se houve **alguma** mudança externa depois da carência. Uma
conexão em que o usuário não tocou no volume não altera nada. O `flush` aqui é
imediato, porque o dispositivo pode não voltar tão cedo.

### 7.3 Persistência

`flush_if_due()` grava o store a cada 2 segundos, e apenas se houver alteração
pendente (`dirty`). Isso agrupa rajadas de mudanças — arrastar o slider de
volume gera dezenas de notificações por segundo — em uma única escrita.

A gravação é atômica em relação a falhas do processo:

1. monta o arquivo inteiro em memória,
2. escreve em `devices.tsv.tmp`,
3. `rename` sobre `devices.tsv`.

Se o programa for morto no meio, o arquivo antigo continua válido.

---

## 8. Formato do store

Arquivo: `%LOCALAPPDATA%\bt-volume\devices.tsv`, UTF-8, separado por tabs.

```text
# bt-volume store v1 (level: 0..10000)
{0.0.0.00000000}.{058eb3bd-2683-49b5-ad02-dff57588b985}	9600	Fone de ouvido do headset (联想thinkplus-GM2 pro)
```

- Primeira linha é comentário (`#`), ignorada na leitura.
- Campos: `endpoint_id` **TAB** `level` **TAB** `friendly_name`.
- `level` vai de `0` a `10000`, em passos de 0,01%. Usar inteiro evita
  imprecisão de ponto flutuante e deixa o arquivo legível/editável à mão.
- Nome com tab, CR ou LF é sanitizado para espaço; nome vazio vira
  `Bluetooth audio device`.

Conversões em `store.rs`:

```rust
units_from_scalar(0.42) == 4200   // arredonda
scalar_from_units(4200) == 0.42
percent(4250)           == 43     // arredonda para exibição
```

### 8.1 Recuperação após re-pareamento

O id do endpoint é estável enquanto o pareamento existe, mas pode mudar se o
dispositivo for removido e pareado de novo. Para não perder o nível,
`Store::lookup()` faz:

1. Busca exata pelo `id`.
2. Se não achar, procura registros com o **mesmo nome amigável**
   (comparação *case-insensitive*).
3. Se houver **exatamente um** com esse nome, devolve o nível e o id antigo.
4. Se houver mais de um, considera ambíguo e devolve `None` — melhor não
   restaurar nada do que restaurar o volume de outro fone com nome parecido.

Quando o id antigo é encontrado, `Engine::stored_level()` chama `store.adopt()`,
que remove o registro velho e cria o novo com o id atual, registrando no log
“*moved saved volume from old endpoint id ... to ...*”.

---

## 9. Interface na bandeja

`tray.rs` não usa framework de GUI. Ele registra uma `WNDCLASSW`
(`BtVolumeTrayWindow`), cria uma janela invisível e usa `Shell_NotifyIconW`
diretamente.

| Elemento | Comportamento |
| --- | --- |
| Ícone | `IDI_APPLICATION` (ícone padrão do Windows) |
| Clique esquerdo / duplo | abre a janela de dispositivos |
| Clique direito | menu de contexto |
| Tooltip | atualizado a cada 1 s; mostra até um dispositivo, ou a contagem |
| Balões | consumidos de `Shared::notes`, uma notificação por segundo |

O menu oferece:

- **Show status** — balão com até 4 dispositivos e seus volumes atuais.
- **Devices window…** — abre a janela do egui (seção 10).
- **Restore saved volumes** — executa o comando `RestoreAll`
  (`AudioRequest::RestoreAll`).
- **Open data folder** / **Open log file** — `ShellExecuteW("open", ...)`.
- **Start with Windows** — marcado com `MF_CHECKED` se a chave existir; alterna
  o valor ao clicar.
- **Exit** — destrói a janela, o que encerra o loop de mensagens e o programa.

Detalhes importantes:

- **Reinício do Explorer**: quando o Explorer reinicia, todos os ícones da
  bandeja somem. O programa registra `RegisterWindowMessageW("TaskbarCreated")`
  e, ao receber essa mensagem, re-adiciona o ícone.
- **Ícone teimoso na inicialização**: `add_icon()` tenta `NIM_ADD` até 10 vezes,
  com 500 ms de espera. Isso evita o caso em que o programa inicia junto com o
  Windows, antes de a bandeja estar pronta.
- **Menu fantasma**: antes de mostrar o menu, chama `SetForegroundWindow` e,
  depois, envia `WM_NULL`. Sem esse par, o menu pode ficar aberto após o
  primeiro clique fora dele (bug clássico de `TrackPopupMenu`).
- **Estado compartilhado**: `Shared::devices` contém `{ name, level_pct }`; o
  motor republica depois de cada scan e de cada restauração. A thread da
  bandeja só lê.
- **Clique abre a janela**: se `ui::is_available()` for falso (a janela já falhou
  antes), o clique volta a mostrar o balão de status.

### 9.1 Autostart

`autostart.rs` opera apenas em `HKCU`, sem exigir administrador:

```
HKCU\Software\Microsoft\Windows\CurrentVersion\Run
    BtVolume = "C:\caminho\bt-volume.exe"
```

`is_enabled()` usa `RegQueryValueExW` só para verificar existência; `set_enabled(true)`
grava o caminho **entre aspas** (para suportar pastas com espaço) e
`set_enabled(false)` remove o valor. O caminho gravado é o
`std::env::current_exe()` do momento do clique, então mover o `.exe` depois exige
reconfigurar.

---

## 10. Janela de dispositivos (egui)

A janela é opcional e existe apenas enquanto está aberta. Ela é construída com
`eframe`/`egui` e roda na thread principal, porque o `winit` exige que o loop de
eventos esteja lá.

### 10.1 Ciclo de vida

```
clique na bandeja
    └─ ui::request_show() ──► canal de pedidos
                                 └─ ui::run_loop() (main)
                                       └─ eframe::run_native(...)   ← bloqueia
                                             └─ usuário fecha a janela
                                                   └─ run_loop volta a esperar
```

- `ui::init()` cria o canal `mpsc` de pedidos (`Show`/`Exit`) e guarda
  `Arc<Shared>`, o `Sender<AudioRequest>` e o `Arc<CommandRegistry>` em
  `OnceLock`s.
- `ui::run_loop()` bloqueia em `recv_timeout(250 ms)`. Ao receber `Show`, chama
  `run_window()`, que entra no eframe e só retorna quando a janela fecha.
- `request_show()` com a janela já aberta não abre outra: manda
  `ViewportCommand::Focus` pelo `egui::Context` guardado.
- `request_exit()` (chamado pela bandeja ao sair) manda `Close` para a janela e
  enfileira `Exit`; o `run_loop` termina e o processo encerra.
- O `--show` na linha de comando apenas enfileira um `Show` antes do loop.

Por que não deixar a janela criada e escondida? Porque o eframe/egui 0.37 chama
`window.set_visible(true)` depois do primeiro frame, ignorando
`ViewportBuilder::with_visible(false)`. Criar sob demanda evita esse flash e
mantém o boot “só bandeja” sem contexto OpenGL.

### 10.2 Dados exibidos

A cada frame (repintado a 250 ms) a janela lê duas fontes:

- `Shared::saved_devices()` — instantâneo do store: `id`, `name`, `units`.
- `Shared::devices` — dispositivos conectados no momento: `name`, `level_pct`.

O pareamento é por nome (`eq_ignore_ascii_case`), na função `row_for()`, que
devolve `(percentual_salvo, Option<percentual_atual>)`. A tabela mostra:

| Coluna | Conteúdo |
| --- | --- |
| Device | nome truncado em 38 caracteres; tooltip com nome completo + id do endpoint |
| Saved | `store::percent(units)` |
| Now | volume atual, ou `—` quando desconectado |
| State | `connected` (verde) ou `saved` (cinza) |

Botões: **Restore saved volumes** (`ids::RESTORE_ALL`), **Open data folder**,
**Open log** (ambos executam comandos do registro) e **Hide** (fecha a janela,
sem encerrar o programa).

### 10.3 Fontes e nomes não latinos

O egui traz uma fonte que cobre latim, mas não CJK. Nomes como
`Fone de ouvido do headset (联想thinkplus-GM2 pro)` apareceriam como caixas.
Por isso `load_fonts_if_needed()` verifica se algum nome tem caractere não ASCII
e, nesse caso, carrega a primeira fonte existente da lista:

```text
C:\Windows\Fonts\msyh.ttc   (Microsoft YaHei, índice 0)
C:\Windows\Fonts\msyh.ttf
C:\Windows\Fonts\simhei.ttf
C:\Windows\Fonts\simsun.ttc
C:\Windows\Fonts\segoeui.ttf
```

Ela entra como *fallback* no fim das famílias `Proportional` e `Monospace`:
texto latino continua na fonte padrão, e os ideogramas caem na fonte do sistema.
Se nenhuma existir, a janela abre mesmo assim (com caixas) e o log registra o
caso. Como o download de 19 MB do `.ttc` só acontece quando necessário, um
store só com nomes ASCII não paga esse custo.

### 10.4 Falhas

Se o contexto OpenGL não puder ser criado, `run_window()` registra o erro, mostra
uma caixa de mensagem e marca `UNAVAILABLE`. A bandeja continua funcionando e o
item **Devices window…** passa a responder com um balão de aviso; a restauração
de volume não depende da interface.

---

## 11. Log e diagnóstico

Arquivo: `%LOCALAPPDATA%\bt-volume\bt-volume.log`, uma linha por evento, com
timestamp local em milissegundos.

Eventos registrados:

- início e fim do programa e do motor;
- quantidade de dispositivos carregados do store;
- “tracking Bluetooth device '...' (...)”;
- restauração, salvamento, migração de id, desconexão;
- avisos de divergência de volume após a última tentativa;
- erros de COM, de enumeração, de store e de registro do ícone.

Em builds **debug**, o log também vai para o console. Em **release**, o binário
usa o subsistema GUI (sem janela de console) e o log em arquivo é o único
diagnóstico.

---

## 12. Constantes que controlam o comportamento

Todas em `src/features/audio/policy.rs`, no topo do arquivo:

| Constante | Valor | Efeito |
| --- | --- | --- |
| `SCAN_INTERVAL` | 3 s | frequência do polling de endpoints |
| `LOOP_TICK` | 200 ms | tempo máximo de espera do loop |
| `CONNECT_GRACE` | 3 s | janela em que mudanças não são salvas |
| `RETRY_DELAYS` | 600 / 1400 / 2800 ms | espera somada ao instante da tentativa anterior |
| `MAX_ATTEMPTS` | 4 | total de aplicações por conexão |
| `BT_VOLUME_CONTEXT` | GUID fixo | marca as escritas do próprio programa |

Outros valores: `MAX_LOG_BYTES = 1 MB` (`util.rs`), flush do store a cada 2 s
(`audio.rs`), timer da bandeja de 1 s (`tray.rs`), limite de 32 notificações
enfileiradas (`state.rs`).

Ajustes típicos:

- Fone que ainda “rouba” o volume depois de 3 s → aumentar `RETRY_DELAYS` ou
  `MAX_ATTEMPTS`.
- Restauração atrapalhando quem mexe no volume logo ao conectar → aumentar
  `CONNECT_GRACE`.
- Detecção lenta ao ligar o fone → reduzir `SCAN_INTERVAL` (custa mais CPU).

---

## 13. Build e testes

```powershell
cargo build --release
cargo test
```

`Cargo.toml` define o perfil de release para tamanho e ausência de runtime:

```toml
[profile.release]
opt-level = "z"   # otimiza para tamanho
codegen-units = 1
lto = true
panic = "abort"   # remove o mecanismo de unwinding
strip = true      # remove símbolos
```

`#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` em
`main.rs` faz o release rodar sem console; o debug mantém o console para ver os
logs em tempo real.

Dependências: `windows` (features mínimas listadas em `Cargo.toml`),
`windows-core` (necessário para a macro `#[implement]`, que gera as
implementações COM de `IMMNotificationClient` e `IAudioEndpointVolumeCallback`)
e `eframe` (só a janela de dispositivos; features `glow` e `default_fonts`,
sem `wgpu`). O binário final fica em torno de 4,7 MB, a maior parte do egui.

### 13.1 Testes

`cargo test` roda 34 testes unitários que não precisam de dispositivo Bluetooth,
de janela ou de sessão interativa. A lógica foi separada do COM justamente para
isso: `is_bluetooth`, `next_retry_delay` e `outside_grace` são funções puras
usadas pelo código real.

| Módulo | O que fica travado |
| --- | --- |
| `features::store` | leitura do TSV (comentários, linhas vazias/malformadas, nível fora da faixa, nome vazio), detecção de mudança real, busca por id, busca por nome único, ambiguidade, migração de id, gravação atômica (`.tmp` some), ida-e-volta em UTF-8, conversões e arredondamentos |
| `features::audio::policy` | `BTH` no `InstanceId` e no enumerador (inclusive o caso real de `InstanceId` vazio), rejeição de USB/HDAUDIO/SWD, tabela de retries (0/0,6/2,0/4,8 s, quatro tentativas), bordas da carência de 3 s |
| `commands` | todas as features registradas e ids únicos |
| `core::command` | busca e execução de comandos por id |
| `core::state` | instantâneo de `saved_devices`, limite de 32 notificações |
| `core::util` | strings wide com NUL, buffer fixo sem estouro, *surrogate pairs* não partidos, truncamento por caracteres |
| `features::ui` | pareamento nome salvo ↔ nome conectado, exibição de `—` quando desconectado |

Os testes de store usam uma pasta temporária própria e a removem no fim; não
tocam em `%LOCALAPPDATA%\bt-volume`.

---

## 14. Limitações conhecidas

- **Precisa estar rodando** para restaurar. Use “Start with Windows”.
- **Um volume por endpoint**: um headset que expõe A2DP *e* mãos-livres tem dois
  endpoints e, portanto, dois níveis independentes.
- **Disputa pós-conexão**: se o fone reimpor o volume depois da quarta
  tentativa, o valor final será o do fone. O log registra a divergência.
- **Janela de restauração**: até a última tentativa (4,8 s), mudanças feitas
  pelo usuário podem ser sobrescritas.
- **Fallback por nome** só funciona se o nome for único no store.
- **Apenas reprodução** (`eRender`) e apenas endpoints ativos; dispositivos
  com fio são ignorados.
- **Sem ícone personalizado**: usa `IDI_APPLICATION`; não há recurso `.ico`
  embutido.
- **Janela precisa de OpenGL**: se o contexto não puder ser criado, a interface
  fica indisponível, mas bandeja e restauração continuam funcionando.
- **Estado é local ao usuário**: store, log e autostart ficam no perfil de quem
  executa.

---

## 15. Resumo de uma conexão, em uma frase

> O motor vê um endpoint `BTH` novo, procura o nível salvo pelo id (ou pelo nome
> único), aplica esse nível em 0/0,6/2,0/4,8 s marcando as escritas com um GUID
> privado, ignora o sincronismo do fone nos primeiros 3 s e passa a gravar em
> `devices.tsv` qualquer mudança posterior do usuário.
