# FlowTracer — Especificação Técnica

## 1. Resumo Executivo

**FlowTracer** é uma ferramenta CLI que reconstrói e visualiza o fluxo de execução de requisições em aplicações backend a partir de logs existentes. O foco principal é dar **visibilidade clara do caminho percorrido no código** durante uma requisição e **identificação imediata de erros** (catch, throw, exceptions).

---

## 2. Escolha da Tecnologia: Rust

### Justificativa

| Critério | Rust | Go | Node.js |
|---|---|---|---|
| Performance em parsing de texto | Excelente | Boa | Moderada |
| Consumo de memória em streaming | Mínimo | Baixo | Alto (GC) |
| Distribuição (binário único) | Sim | Sim | Não (runtime) |
| Ecossistema para CLI | Excelente (clap, regex) | Bom | Bom |
| Processamento de arquivos grandes | Excelente (zero-copy) | Bom | Limitado |
| Precedentes no domínio | ripgrep, vector, bat | stern, loki | - |

**Rust é a escolha ideal** porque:

1. **Performance**: Log parsing é I/O e CPU intensivo. Rust processa texto na velocidade de ripgrep — ordens de magnitude mais rápido que alternativas. Isso importa quando se analisa gigabytes de logs de produção.
2. **Streaming com memória constante**: Rust permite processar logs via stdin (pipe) linha a linha sem carregar tudo em memória, usando iteradores lazy e zero-copy parsing.
3. **Binário único sem dependências**: Distribui-se um único executável. Sem runtime, sem instalação de dependências, sem `node_modules`. Funciona em qualquer servidor de produção.
4. **Ecossistema maduro para o domínio**: `regex` (mesma engine do ripgrep), `clap` (CLI args), `serde` (serialização), `crossterm`/`ratatui` (terminal UI com cores).
5. **Precedente comprovado**: As ferramentas CLI de referência no mercado (ripgrep, fd, bat, delta, vector.dev) são todas Rust, validando a escolha para este domínio.

### Crates Principais

| Crate | Função |
|---|---|
| `clap` | Parsing de argumentos CLI |
| `regex` | Extração de padrões dos logs |
| `serde` + `serde_json` | Serialização de traces (JSON output) |
| `chrono` | Parsing de timestamps |
| `crossterm` | Controle de terminal (cores, formatação) |
| `tokio` | Async I/O para streaming e watch mode |
| `thiserror` / `anyhow` | Tratamento de erros |
| `uuid` | Geração de IDs internos para spans |

---

## 3. Arquitetura

```
                    ┌──────────────────────────────────────────────┐
                    │               FlowTracer CLI                 │
                    └──────────────┬───────────────────────────────┘
                                   │
                    ┌──────────────▼───────────────────────────────┐
                    │            Input Layer                        │
                    │  ┌─────────┐ ┌─────────┐ ┌───────────────┐  │
                    │  │  stdin  │ │  file   │ │  watch (tail) │  │
                    │  └────┬────┘ └────┬────┘ └──────┬────────┘  │
                    │       └───────────┼─────────────┘            │
                    └───────────────────┼──────────────────────────┘
                                        │  Iterator<Line>
                    ┌───────────────────▼──────────────────────────┐
                    │          Log Parser (plugável)                │
                    │                                               │
                    │  ┌─────────────────────────────────────────┐ │
                    │  │ Detecta formato automaticamente:        │ │
                    │  │  • plain text                           │ │
                    │  │  • JSON structured                      │ │
                    │  │  • formato customizado (via config)     │ │
                    │  └─────────────────────────────────────────┘ │
                    └───────────────────┼──────────────────────────┘
                                        │  Vec<LogEvent>
                    ┌───────────────────▼──────────────────────────┐
                    │         Event Classifier                      │
                    │                                               │
                    │  Classifica cada evento como:                 │
                    │  • ENTRY   (entrada em função/método)        │
                    │  • EXIT    (saída de função/método)          │
                    │  • ERROR   (throw, catch, exception)         │
                    │  • LOG     (log genérico informativo)        │
                    │  • ASYNC   (publish, consume, dispatch)      │
                    └───────────────────┼──────────────────────────┘
                                        │
                    ┌───────────────────▼──────────────────────────┐
                    │       Request Grouper                         │
                    │                                               │
                    │  Agrupa eventos por requisição usando:        │
                    │  1. request_id / trace_id (explícito)        │
                    │  2. thread_id / correlation_id               │
                    │  3. Heurística temporal (threshold)          │
                    └───────────────────┼──────────────────────────┘
                                        │  HashMap<RequestId, Vec<Event>>
                    ┌───────────────────▼──────────────────────────┐
                    │        Trace Builder                          │
                    │                                               │
                    │  Reconstrói a árvore de execução:             │
                    │  • Call stack via ENTRY/EXIT                  │
                    │  • Stack trace parsing (exceções)            │
                    │  • Detecção de profundidade (indentação)     │
                    │  • Cálculo de duração entre spans            │
                    └───────────────────┼──────────────────────────┘
                                        │  Trace (árvore de Spans)
                    ┌───────────────────▼──────────────────────────┐
                    │        Renderer (plugável)                    │
                    │                                               │
                    │  ┌──────┐ ┌──────┐ ┌──────┐ ┌────────────┐ │
                    │  │ tree │ │ flat │ │ json │ │  flamegraph │ │
                    │  └──────┘ └──────┘ └──────┘ └────────────┘ │
                    └──────────────────────────────────────────────┘
```

### Fluxo de Dados

```
Linha de log (string)
    │
    ▼
LogEvent {
    timestamp: Option<DateTime>,
    level: LogLevel,
    message: String,
    request_id: Option<String>,
    trace_id: Option<String>,
    thread_id: Option<String>,
    source: Option<String>,        // arquivo:linha quando disponível
    raw: String,                   // linha original preservada
}
    │
    ▼
ClassifiedEvent {
    event: LogEvent,
    kind: EventKind,               // Entry | Exit | Error | Log | Async
    function_name: Option<String>, // nome da função extraído
    error_detail: Option<ErrorDetail>,
}
    │
    ▼
Trace {
    request_id: String,
    root: Span,
    total_duration: Option<Duration>,
    has_error: bool,
}

Span {
    id: String,
    name: String,                  // nome da função/operação
    kind: SpanKind,
    start_time: Option<DateTime>,
    end_time: Option<DateTime>,
    duration: Option<Duration>,
    children: Vec<Span>,
    error: Option<ErrorDetail>,
    metadata: HashMap<String, String>,
}

ErrorDetail {
    message: String,
    error_type: ErrorType,         // Throw | Catch | Exception | Panic
    stack_trace: Option<Vec<StackFrame>>,
    source_location: Option<String>,
}
```

---

## 4. Estruturas Centrais

### 4.1 LogEvent — Evento bruto parseado

```rust
pub struct LogEvent {
    pub timestamp: Option<chrono::NaiveDateTime>,
    pub level: LogLevel,
    pub message: String,
    pub request_id: Option<String>,
    pub trace_id: Option<String>,
    pub thread_id: Option<String>,
    pub source_location: Option<String>,
    pub raw_line: String,
    pub line_number: usize,
}

pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
    Unknown,
}
```

### 4.2 Span — Nó da árvore de execução

```rust
pub struct Span {
    pub id: Uuid,
    pub name: String,
    pub kind: SpanKind,
    pub start_time: Option<NaiveDateTime>,
    pub end_time: Option<NaiveDateTime>,
    pub children: Vec<Span>,
    pub error: Option<ErrorDetail>,
    pub events: Vec<LogEvent>,
    pub metadata: HashMap<String, String>,
}

pub enum SpanKind {
    Function,
    HttpRequest,
    DatabaseQuery,
    MessagePublish,
    MessageConsume,
    ExternalCall,
    Unknown,
}
```

### 4.3 ErrorDetail — Informação de erro enriquecida

```rust
pub struct ErrorDetail {
    pub message: String,
    pub error_type: ErrorType,
    pub stack_trace: Option<Vec<StackFrame>>,
    pub source_location: Option<String>,
    pub caught: bool,  // true se o erro foi capturado em um catch
}

pub enum ErrorType {
    Throw,
    Catch,
    Exception,
    Panic,
    Rejection,   // Promise rejection
    Timeout,
    Unknown,
}

pub struct StackFrame {
    pub function_name: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}
```

### 4.4 Trace — Trace completo de uma requisição

```rust
pub struct Trace {
    pub id: String,
    pub request_id: Option<String>,
    pub trace_id: Option<String>,
    pub root: Span,
    pub start_time: Option<NaiveDateTime>,
    pub end_time: Option<NaiveDateTime>,
    pub total_duration: Option<Duration>,
    pub has_error: bool,
    pub error_count: usize,
    pub span_count: usize,
}
```

---

## 5. Módulos e Responsabilidades

### 5.1 `input` — Leitura de logs

Responsável por fornecer um iterador de linhas independente da fonte.

```
src/input/
├── mod.rs
├── stdin.rs       # leitura de stdin (pipe mode)
├── file.rs        # leitura de arquivo(s)
└── watch.rs       # tail -f mode (watch contínuo)
```

**Comportamento:**
- `stdin`: Lê linha a linha via `BufRead`. Sem buffering excessivo para modo streaming.
- `file`: Lê arquivo completo ou range de linhas. Suporta glob patterns para múltiplos arquivos.
- `watch`: Usa `notify` crate para detectar mudanças e processar novas linhas incrementalmente.

### 5.2 `parser` — Parsing de linhas de log

Converte strings brutas em `LogEvent`. Suporta múltiplos formatos via trait `LogParser`.

```
src/parser/
├── mod.rs
├── auto_detect.rs   # detecta formato automaticamente
├── plain.rs         # logs em texto plano
├── json.rs          # logs estruturados (JSON lines)
├── custom.rs        # formato definido via config/regex
└── stacktrace.rs    # parser especializado para stack traces
```

**Trait principal:**

```rust
pub trait LogParser: Send + Sync {
    fn parse_line(&self, line: &str, line_number: usize) -> Option<LogEvent>;
    fn detect_format(sample: &[&str]) -> bool;
}
```

**Detecção automática de formato:**
1. Lê as primeiras 10 linhas como amostra
2. Tenta JSON parse → se sucesso, usa `JsonParser`
3. Tenta matching contra padrões comuns de log → `PlainParser` com regex detectado
4. Fallback: trata cada linha como mensagem crua

**Padrões de log suportados nativamente:**

| Padrão | Regex | Exemplo |
|---|---|---|
| ISO timestamp + level | `(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2})\s*\[?(INFO\|WARN\|ERROR\|DEBUG)\]?\s*(.*)` | `2026-03-12 10:10:01 [INFO] message` |
| Level prefix | `\[(INFO\|WARN\|ERROR\|DEBUG)\]\s*(.*)` | `[ERROR] No provider found` |
| Spring Boot | `(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3})\s+(INFO\|WARN\|ERROR)\s+\d+\s+---\s+\[.*\]\s+(\S+)\s*:\s*(.*)` | Spring Boot default format |
| Serilog | JSON com `@t`, `@l`, `@m` | Serilog compact JSON |
| Node.js/pino | JSON com `level`, `msg`, `time` | Pino structured logs |
| Python logging | `(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2},\d{3})\s+-\s+(\w+)\s+-\s+(\w+)\s+-\s+(.*)` | Python logging default |

### 5.3 `classifier` — Classificação de eventos

Analisa o conteúdo de cada `LogEvent` e determina se é uma entrada de função, saída, erro, etc.

```
src/classifier/
├── mod.rs
├── patterns.rs      # padrões regex para classificação
├── error.rs         # detecção e enriquecimento de erros
└── rules.rs         # regras customizáveis via config
```

**Padrões de detecção de ENTRY (entrada em função):**

```
Executing <name>
Enter <name>
Entering <name>
Starting <name>
--> <name>
<name> started
Handling <name>
Processing <name>
Calling <name>
```

**Padrões de detecção de ERROR:**

```
[ERROR] <message>
[FATAL] <message>
Exception: <message>
throw new <ErrorType>(<message>)
caught exception: <message>
Error: <message>
PANIC: <message>
Unhandled rejection: <message>
failed to <action>: <message>
at <function> (<file>:<line>:<col>)       ← stack trace frame
   at <function> (<file>:<line>:<col>)    ← stack trace continuation
Caused by: <message>                       ← chained exception
```

**Padrões de detecção de CATCH (erro capturado):**

```
caught: <message>
handled error: <message>
recovering from: <message>
fallback triggered: <message>
retry attempt <n>: <message>
```

**Classificação com prioridade:**
1. Se contém stack trace → `Error` com `stack_trace` parseado
2. Se nível é ERROR/FATAL → `Error`
3. Se match com padrão de entry → `Entry`
4. Se match com padrão de exit → `Exit`
5. Se match com padrão async → `Async`
6. Default → `Log`

### 5.4 `grouper` — Agrupamento por requisição

```
src/grouper/
├── mod.rs
├── by_id.rs         # agrupamento por request_id/trace_id
├── by_thread.rs     # agrupamento por thread_id
└── by_time.rs       # agrupamento heurístico por proximidade temporal
```

**Estratégia de agrupamento (em ordem de prioridade):**

1. **Explicit ID**: Se `request_id` ou `trace_id` presente nos eventos, agrupa por esse ID.
2. **Thread ID**: Se `thread_id` presente, agrupa eventos do mesmo thread.
3. **Temporal heuristic**: Agrupa eventos dentro de uma janela temporal configurável (default: 500ms). Um novo grupo inicia quando:
   - Gap temporal entre eventos excede o threshold
   - Um novo padrão de ENTRY é detectado sem EXIT correspondente do grupo anterior

**Merge de grupos:** Quando um `trace_id` aparece em eventos de grupos diferentes (ex: fluxo assíncrono entre serviços), os grupos são mesclados num único trace com spans paralelos.

### 5.5 `trace_builder` — Construção da árvore de execução

```
src/trace_builder/
├── mod.rs
├── stack.rs         # reconstrução via call stack (ENTRY/EXIT pairs)
├── stacktrace.rs    # reconstrução via stack traces de exceções
├── heuristic.rs     # reconstrução heurística por indentação/ordem
└── merge.rs         # merge de traces parciais
```

**Algoritmo de reconstrução (call stack):**

```
stack = []
root = Span::new("root")

for event in events:
    match event.kind:
        Entry(name):
            span = Span::new(name)
            stack.last().add_child(span)
            stack.push(span)

        Exit(name):
            if stack.last().name == name:
                stack.last().end_time = event.timestamp
                stack.pop()

        Error(detail):
            stack.last().error = detail
            // propaga has_error para todos os pais

        Log:
            stack.last().add_event(event)
```

**Algoritmo de reconstrução (stack trace):**

```
// Stack trace vem de baixo para cima (frame mais profundo primeiro)
frames = parse_stack_trace(error_log)
frames.reverse()  // agora do mais externo para o mais interno

root = Span::from_frame(frames[0])
current = root

for frame in frames[1..]:
    child = Span::from_frame(frame)
    current.add_child(child)
    current = child

// O último span recebe o erro
current.error = error_detail
```

**Heurística de profundidade:**
Quando não há ENTRY/EXIT explícitos, a ferramenta infere profundidade por:
1. Indentação do log (se consistente)
2. Prefixos como `→`, `└─`, `├─`
3. Padrões de nomes (Controller → Service → Repository)

### 5.6 `renderer` — Visualização

```
src/renderer/
├── mod.rs
├── tree.rs          # visualização em árvore (padrão)
├── flat.rs          # visualização linear/flat
├── json.rs          # output JSON estruturado
├── compact.rs       # visualização compacta (resumo)
└── colors.rs        # paleta de cores e formatação terminal
```

**Trait principal:**

```rust
pub trait Renderer {
    fn render(&self, trace: &Trace, config: &RenderConfig) -> String;
}
```

---

## 6. Detecção e Exibição de Erros (Foco Principal)

A detecção de erros é o diferencial principal do FlowTracer. O sistema deve tornar **impossível não ver um erro** no output.

### 6.1 Categorias de Erro

| Categoria | Detecção | Símbolo | Cor |
|---|---|---|---|
| **Throw** | `throw new`, `raise`, `panic!` | `⚡ THROW` | Vermelho brilhante |
| **Catch** | `catch`, `caught`, `handled`, `rescue` | `🛡 CATCH` | Amarelo |
| **Exception não tratada** | Stack trace sem catch correspondente | `💥 UNCAUGHT` | Vermelho + fundo |
| **Timeout** | `timeout`, `timed out`, `deadline exceeded` | `⏱ TIMEOUT` | Magenta |
| **Rejeição** | `unhandled rejection`, `promise rejected` | `💥 REJECTED` | Vermelho |
| **Retry** | `retry`, `attempt`, `retrying` | `🔄 RETRY` | Cyan |

### 6.2 Output de erro no trace

Quando um erro é detectado, o trace exibe:

```
CreateOrderController                                    12ms
├─ GetUser                                                3ms
├─ GetCart                                                2ms
├─ CreateInvoice                                          5ms
│  └─ GetProvider
│     └─ ⚡ THROW: No provider found with name "paypau"
│        │
│        │  PaymentService.GetProvider (payment_service.rs:42)
│        │  InvoiceService.CreateInvoice (invoice_service.rs:87)
│        │  OrderController.CreateOrder (order_controller.rs:15)
│        │
│        └─ 🛡 CATCH em CreateOrderController
│           → Fallback: usando provider padrão "stripe"
```

### 6.3 Resumo de erros

Ao final de cada trace com erro, exibe um resumo:

```
─── Error Summary ────────────────────────────────────
  ⚡ 1 throw   │ PaymentService.GetProvider → "No provider found with name paypau"
  🛡 1 catch   │ CreateOrderController → fallback ativado
  💥 0 uncaught
───────────────────────────────────────────────────────
```

### 6.4 Propagação visual de erro

Quando um erro ocorre em um span filho, **toda a cadeia de spans até o root é marcada visualmente** com cor vermelha na borda, permitindo rastrear instantaneamente o caminho do erro:

```
❌ CreateOrderController                                 12ms
│  ├─ GetUser                                             3ms
│  ├─ GetCart                                             2ms
│  └─❌ CreateInvoice                                     5ms
│     └─❌ GetProvider
│        └─ ⚡ THROW: No provider found with name "paypau"
```

O prefixo `❌` (renderizado em vermelho no terminal) propaga do ponto do erro até o root, tornando o caminho do erro imediatamente visível mesmo em traces com dezenas de spans.

---

## 7. Interface CLI

### 7.1 Comandos e flags

```
USAGE:
    flowtracer [OPTIONS] [FILE]...
    <command> | flowtracer [OPTIONS]

ARGS:
    [FILE]...    Arquivo(s) de log para analisar. Omitir para ler de stdin.

OPTIONS:
    -r, --request <ID>         Filtrar por request ID
    -t, --trace <ID>           Filtrar por trace ID
    -e, --errors-only          Mostrar apenas traces que contêm erros
    -f, --format <FORMAT>      Formato de saída [default: tree]
                               [possíveis: tree, flat, json, compact]
    -w, --watch                Modo watch (tail -f contínuo)
    -n, --last <N>             Mostrar apenas os últimos N traces
    -g, --grep <PATTERN>       Filtrar traces que contêm o padrão
    -d, --max-depth <N>        Profundidade máxima da árvore
    -c, --config <FILE>        Arquivo de configuração
        --no-color             Desabilitar cores no output
        --time-threshold <MS>  Threshold para agrupamento temporal [default: 500]
        --show-raw             Mostrar linhas de log originais junto ao trace
        --stats                Mostrar estatísticas ao final
    -v, --verbose              Output verboso (inclui logs classificados como genéricos)
    -h, --help                 Mostrar ajuda
    -V, --version              Mostrar versão
```

### 7.2 Exemplos de uso

**Analisar arquivo de log:**
```bash
flowtracer app.log
```

**Pipe de stdout:**
```bash
cargo run -- serve 2>&1 | flowtracer
```

**Filtrar por request ID:**
```bash
flowtracer --request abc-123 app.log
```

**Mostrar apenas erros em modo watch:**
```bash
tail -f /var/log/app.log | flowtracer --errors-only --watch
```

**Output JSON para integração com outras ferramentas:**
```bash
flowtracer --format json app.log | jq '.traces[] | select(.has_error)'
```

**Analisar múltiplos arquivos (fluxo distribuído):**
```bash
flowtracer api.log worker.log email-service.log --trace abc-123
```

**Estatísticas de erro:**
```bash
flowtracer --errors-only --stats production.log
```

### 7.3 Output esperado por formato

**`--format tree`** (padrão):
```
Trace abc-123  ─────────────────────────────────  12ms  ❌ 1 error

❌ CreateOrderController                                 12ms
   ├── GetUser                                            3ms
   ├── GetCart                                            2ms
   └──❌ CreateInvoice                                    5ms
       └──❌ GetProvider
           └── ⚡ THROW: No provider found with name "paypau"
               at PaymentService.GetProvider (payment_service.rs:42)

─── Error Summary ─────────────────────────────────────
  ⚡ THROW │ PaymentService.GetProvider
          │ "No provider found with name paypau"
────────────────────────────────────────────────────────
```

**`--format flat`**:
```
[abc-123] CreateOrderController → GetUser → GetCart → CreateInvoice → GetProvider → ⚡ THROW: No provider found
```

**`--format compact`**:
```
abc-123  12ms  ❌ CreateOrderController > ... > GetProvider > THROW: No provider found
def-456   8ms  ✅ ListProductsController > GetProducts > FormatResponse
ghi-789  45ms  ❌ PaymentController > ChargeCard > ⏱ TIMEOUT: gateway timeout
```

**`--format json`**:
```json
{
  "trace_id": "abc-123",
  "duration_ms": 12,
  "has_error": true,
  "error_count": 1,
  "span_count": 5,
  "root": {
    "name": "CreateOrderController",
    "duration_ms": 12,
    "has_error": true,
    "children": [
      {
        "name": "GetUser",
        "duration_ms": 3,
        "has_error": false,
        "children": []
      },
      {
        "name": "GetCart",
        "duration_ms": 2,
        "has_error": false,
        "children": []
      },
      {
        "name": "CreateInvoice",
        "duration_ms": 5,
        "has_error": true,
        "children": [
          {
            "name": "GetProvider",
            "duration_ms": null,
            "has_error": true,
            "error": {
              "type": "throw",
              "message": "No provider found with name \"paypau\"",
              "stack_trace": [
                "PaymentService.GetProvider (payment_service.rs:42)",
                "InvoiceService.CreateInvoice (invoice_service.rs:87)",
                "OrderController.CreateOrder (order_controller.rs:15)"
              ]
            },
            "children": []
          }
        ]
      }
    ]
  }
}
```

---

## 8. Arquivo de Configuração

O FlowTracer pode ser customizado via arquivo `.flowtracer.toml` no diretório corrente ou via `--config`.

```toml
# .flowtracer.toml

[parser]
# Formato de log (auto, plain, json, custom)
format = "auto"

# Regex customizado para parsing de linhas de log
# Grupos nomeados: timestamp, level, message, request_id, thread_id
custom_pattern = '''
(?P<timestamp>\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3})\s+
\[(?P<level>\w+)\]\s+
\[(?P<request_id>[\w-]+)\]\s+
(?P<message>.*)
'''

# Formato do timestamp
timestamp_format = "%Y-%m-%d %H:%M:%S%.3f"

[classifier]
# Padrões adicionais para detecção de entrada em função
entry_patterns = [
    "Executing method {name}",
    "Handler: {name}",
    "UseCase {name} started",
]

# Padrões adicionais para detecção de erro
error_patterns = [
    "FAILURE: {message}",
    "Operação falhou: {message}",
]

# Padrões para detecção de catch
catch_patterns = [
    "Error handled by {name}",
    "Fallback activated: {message}",
]

# Padrões para detecção de saída de função
exit_patterns = [
    "{name} completed",
    "{name} finished in {duration}ms",
]

[grouper]
# Estratégia de agrupamento: auto, request_id, trace_id, thread_id, temporal
strategy = "auto"

# Campo do log que contém o ID de agrupamento (para JSON logs)
request_id_field = "requestId"
trace_id_field = "traceId"
thread_id_field = "threadId"

# Threshold temporal para agrupamento heurístico
time_threshold_ms = 500

[renderer]
# Formato padrão: tree, flat, json, compact
default_format = "tree"

# Profundidade máxima (0 = sem limite)
max_depth = 0

# Mostrar duração dos spans
show_duration = true

# Mostrar timestamp dos eventos
show_timestamp = false

# Mostrar linhas raw do log junto ao trace
show_raw_lines = false

# Cores habilitadas
colors = true

[filter]
# Níveis de log a incluir
levels = ["INFO", "WARN", "ERROR", "FATAL"]

# Padrões de função a ignorar (noise reduction)
ignore_patterns = [
    "HealthCheck",
    "MetricsEndpoint",
    "LoggingMiddleware",
]
```

---

## 9. Estrutura do Projeto

```
flowtracer/
├── Cargo.toml
├── Cargo.lock
├── .flowtracer.toml              # configuração default de exemplo
├── README.md
├── src/
│   ├── main.rs                   # entry point, CLI setup
│   ├── lib.rs                    # re-exports públicos
│   ├── cli.rs                    # definição de argumentos (clap)
│   ├── config.rs                 # parsing de .flowtracer.toml
│   │
│   ├── input/
│   │   ├── mod.rs
│   │   ├── stdin.rs
│   │   ├── file.rs
│   │   └── watch.rs
│   │
│   ├── parser/
│   │   ├── mod.rs
│   │   ├── auto_detect.rs
│   │   ├── plain.rs
│   │   ├── json.rs
│   │   ├── custom.rs
│   │   └── stacktrace.rs
│   │
│   ├── classifier/
│   │   ├── mod.rs
│   │   ├── patterns.rs
│   │   ├── error.rs
│   │   └── rules.rs
│   │
│   ├── grouper/
│   │   ├── mod.rs
│   │   ├── by_id.rs
│   │   ├── by_thread.rs
│   │   └── by_time.rs
│   │
│   ├── trace_builder/
│   │   ├── mod.rs
│   │   ├── stack.rs
│   │   ├── stacktrace.rs
│   │   ├── heuristic.rs
│   │   └── merge.rs
│   │
│   ├── renderer/
│   │   ├── mod.rs
│   │   ├── tree.rs
│   │   ├── flat.rs
│   │   ├── json.rs
│   │   ├── compact.rs
│   │   └── colors.rs
│   │
│   └── model/
│       ├── mod.rs
│       ├── event.rs              # LogEvent, ClassifiedEvent
│       ├── span.rs               # Span, SpanKind
│       ├── trace.rs              # Trace
│       └── error.rs              # ErrorDetail, ErrorType, StackFrame
│
└── tests/
    ├── fixtures/
    │   ├── plain_logs.txt
    │   ├── json_logs.jsonl
    │   ├── spring_boot_logs.txt
    │   ├── node_pino_logs.jsonl
    │   ├── with_stacktrace.txt
    │   ├── distributed_flow.txt
    │   └── multi_request.txt
    │
    ├── parser_tests.rs
    ├── classifier_tests.rs
    ├── grouper_tests.rs
    ├── trace_builder_tests.rs
    ├── renderer_tests.rs
    └── integration_tests.rs
```

---

## 10. Fases de Implementação

### Fase 1 — MVP (Core Pipeline)
**Objetivo**: Ler logs de arquivo/stdin, parsear, classificar, agrupar e renderizar em árvore.

| Item | Descrição | Prioridade |
|---|---|---|
| CLI básico | Leitura de arquivo e stdin com `clap` | P0 |
| Plain text parser | Parsing de logs com timestamp + level + message | P0 |
| Classificação de ENTRY e ERROR | Padrões básicos de entrada em função e erros | P0 |
| Agrupamento por request_id | Agrupamento quando ID está presente no log | P0 |
| Agrupamento temporal | Heurística de proximidade temporal | P0 |
| Tree renderer | Visualização em árvore com cores | P0 |
| Propagação visual de erro | Marcar caminho do erro até o root | P0 |
| Error summary | Resumo de erros no final do trace | P0 |

**Entregável**: `flowtracer app.log` funciona com logs simples, mostra árvore com erros destacados.

### Fase 2 — Parsing Avançado
**Objetivo**: Suportar múltiplos formatos e enriquecer detecção de erros.

| Item | Descrição | Prioridade |
|---|---|---|
| JSON parser | Suporte a logs estruturados (pino, serilog) | P1 |
| Stack trace parser | Extrair call chain de stack traces | P1 |
| Auto-detect de formato | Detectar formato automaticamente | P1 |
| Detecção de catch | Identificar erros capturados vs não capturados | P1 |
| Arquivo de configuração | Suporte a `.flowtracer.toml` | P1 |
| Custom patterns | Padrões de classificação configuráveis | P1 |

### Fase 3 — Modos de Uso Avançados
**Objetivo**: Watch mode, filtros avançados, múltiplas saídas.

| Item | Descrição | Prioridade |
|---|---|---|
| Watch mode | `--watch` para tail contínuo | P2 |
| JSON output | `--format json` para integração | P2 |
| Flat e compact renderers | Formatos alternativos de saída | P2 |
| Filtro por padrão | `--grep` para filtrar traces | P2 |
| `--errors-only` | Mostrar apenas traces com erro | P2 |
| `--stats` | Estatísticas de erros e traces | P2 |
| Múltiplos arquivos | Análise de fluxo distribuído | P2 |

### Fase 4 — Inteligência e Extensões
**Objetivo**: Análise avançada e integrações.

| Item | Descrição | Prioridade |
|---|---|---|
| Call graph global | Construção de grafo de chamadas | P3 |
| Detecção de latência | Cálculo de tempo entre spans | P3 |
| Ignore patterns | Filtrar ruído (health checks, etc.) | P3 |
| Suporte a fluxos async | Correlação pub/sub via trace_id | P3 |
| Output para flamegraph | Exportar para formato flamegraph | P3 |

---

## 11. Critérios de Aceite do MVP

1. **Leitura de logs**: Aceita entrada via arquivo e stdin (pipe).
2. **Parsing**: Extrai timestamp, nível e mensagem de logs em texto plano com formatos comuns.
3. **Classificação**: Detecta corretamente entradas em função e erros.
4. **Agrupamento**: Agrupa eventos por `request_id` quando presente, ou por heurística temporal.
5. **Árvore de execução**: Gera uma árvore hierárquica representando o fluxo da requisição.
6. **Erros visíveis**: Erros são destacados com cores, símbolos e propagação visual até o root.
7. **Error summary**: Resumo de erros exibido ao final de cada trace com erro.
8. **Performance**: Processa arquivos de 100MB em menos de 2 segundos.
9. **Zero configuração**: Funciona out-of-the-box sem arquivo de configuração para formatos comuns.

---

## 12. Dependências (Cargo.toml)

```toml
[package]
name = "flowtracer"
version = "0.1.0"
edition = "2021"
description = "Reconstruct execution traces from application logs"
license = "MIT"

[dependencies]
clap = { version = "4", features = ["derive"] }
regex = "1"
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
crossterm = "0.28"
uuid = { version = "1", features = ["v4"] }
thiserror = "2"
anyhow = "1"

[dev-dependencies]
pretty_assertions = "1"
insta = "1"          # snapshot testing para outputs de renderer
tempfile = "3"
```

---

## 13. Decisões Técnicas

| Decisão | Escolha | Justificativa |
|---|---|---|
| Linguagem | Rust | Performance, binário único, ecossistema CLI maduro |
| CLI framework | clap (derive) | Padrão da indústria Rust, tipagem forte, auto-complete |
| Regex engine | regex crate | Mesma engine do ripgrep, extremamente rápida |
| Config format | TOML | Padrão Rust, legível, suporte nativo |
| Output colorido | crossterm | Cross-platform, não depende de ncurses |
| Serialização | serde | Padrão Rust, zero-cost para JSON output |
| Error handling | thiserror + anyhow | thiserror para tipos, anyhow para propagação |
| Testes de output | insta (snapshots) | Garante que outputs não regridem visualmente |
