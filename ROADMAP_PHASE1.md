# FlowTracer — Roadmap de Implementação: Fase 1 (MVP)

## Objetivo

Entregar um binário funcional que lê logs de arquivo ou stdin, reconstrói o fluxo de execução por requisição e exibe uma árvore visual no terminal com **erros claramente destacados e propagados**.

**Resultado esperado ao final da Fase 1:**

```bash
$ flowtracer app.log

Trace abc-123  ─────────────────────────────────  12ms  ❌ 1 error

❌ CreateOrderController                                 12ms
   ├── GetUser                                            3ms
   ├── GetCart                                            2ms
   └──❌ CreateInvoice                                    5ms
       └──❌ GetProvider
           └── ⚡ THROW: No provider found with name "paypau"

─── Error Summary ─────────────────────────────────────
  ⚡ THROW │ GetProvider → "No provider found with name paypau"
────────────────────────────────────────────────────────
```

---

## Pré-requisitos

- Rust toolchain instalado (`rustup`, edição 2021, stable)
- Familiaridade com `clap`, `regex`, `crossterm`

---

## Steps

### Step 1 — Scaffold do Projeto e CLI Básico (concluido)

**Objetivo**: Criar a estrutura do projeto Rust e aceitar entrada via arquivo ou stdin.

**Tarefas:**

1. `cargo init --name flowtracer`
2. Configurar `Cargo.toml` com dependências do MVP (clap, regex, chrono, crossterm, thiserror, anyhow, uuid, serde, serde_json)
3. Criar `src/cli.rs` com a struct de argumentos usando `clap` derive:
   - Argumento posicional: `files: Vec<PathBuf>` (opcional — sem arquivos, lê stdin)
   - Flag: `-r, --request <ID>` para filtrar por request ID
   - Flag: `-e, --errors-only` para mostrar apenas traces com erro
   - Flag: `--no-color` para desabilitar cores
   - Flag: `-n, --last <N>` para limitar quantidade de traces
   - Flag: `--time-threshold <MS>` (default 500)
4. Criar `src/main.rs` que parseia os argumentos e lê linhas da fonte correta (arquivo ou stdin)
5. Criar módulo `src/input/mod.rs` com trait `LogInput` e implementações:
   - `src/input/file.rs` — lê arquivo(s) via `BufReader`
   - `src/input/stdin.rs` — lê stdin via `BufRead`

**Critério de aceite:**
```bash
# Lê de arquivo
flowtracer app.log
# imprime cada linha lida (passthrough para validar I/O)

# Lê de stdin
echo "hello" | flowtracer
# imprime "hello"
```

**Testes:**
- Teste unitário: `file::read_lines` retorna iterador com linhas corretas
- Teste unitário: CLI parseia argumentos corretamente

**Arquivos criados:**
```
Cargo.toml
src/main.rs
src/lib.rs
src/cli.rs
src/input/mod.rs
src/input/file.rs
src/input/stdin.rs
```

---

### Step 2 — Modelo de Dados (concluido)

**Objetivo**: Definir as structs que representam eventos, spans, traces e erros.

**Tarefas:**

1. Criar `src/model/mod.rs` com re-exports
2. Criar `src/model/event.rs`:
   - `LogEvent` — evento bruto parseado (timestamp, level, message, request_id, thread_id, raw_line, line_number)
   - `LogLevel` — enum (Trace, Debug, Info, Warn, Error, Fatal, Unknown)
   - `ClassifiedEvent` — evento classificado (event + kind + function_name + error_detail)
   - `EventKind` — enum (Entry, Exit, Error, Log)
3. Criar `src/model/error.rs`:
   - `ErrorDetail` — informação de erro (message, error_type, source_location)
   - `ErrorType` — enum (Throw, Catch, Exception, Timeout, Rejection, Unknown)
4. Criar `src/model/span.rs`:
   - `Span` — nó da árvore (id, name, kind, start_time, end_time, children, error, events)
   - `SpanKind` — enum (Function, HttpRequest, Unknown)
5. Criar `src/model/trace.rs`:
   - `Trace` — trace completo (id, request_id, root span, duration, has_error, error_count, span_count)

**Critério de aceite:**
- Todas as structs compilam e possuem `Debug`, `Clone` derivados
- `Span` suporta adição de filhos e propagação de `has_error`
- `Trace` calcula `error_count` e `span_count` a partir da árvore

**Testes:**
- Teste unitário: criar Span com filhos e verificar contagem
- Teste unitário: propagar `has_error` de filho para pai
- Teste unitário: `Trace::from_root_span` calcula métricas corretamente

**Arquivos criados:**
```
src/model/mod.rs
src/model/event.rs
src/model/error.rs
src/model/span.rs
src/model/trace.rs
```

---

### Step 3 — Parser de Logs (Plain Text)

**Objetivo**: Converter linhas de texto em `LogEvent`, extraindo timestamp, nível e mensagem.

**Tarefas:**

1. Criar `src/parser/mod.rs` com trait `LogParser`:
   ```rust
   pub trait LogParser: Send + Sync {
       fn parse_line(&self, line: &str, line_number: usize) -> Option<LogEvent>;
   }
   ```
2. Criar `src/parser/plain.rs` com `PlainTextParser` que detecta os padrões:
   - `YYYY-MM-DD HH:MM:SS[.mmm] [LEVEL] message`
   - `YYYY-MM-DDTHH:MM:SS[.mmm] LEVEL message`
   - `[LEVEL] message` (sem timestamp)
   - `LEVEL: message`
3. Extração de `request_id` da mensagem via padrões:
   - `RequestId=<id>`
   - `[<id>]` no início da mensagem (quando parece UUID ou ID alfanumérico)
   - `request_id:<id>`
   - `traceId=<id>`
4. Extração de `thread_id` via padrões:
   - `[Thread <id>]`
   - `[<thread-name>]`

**Critério de aceite:**
```
Input:  "2026-03-12 10:10:01 [INFO] RequestId=abc-123 Executing CreateOrderController"
Output: LogEvent {
    timestamp: Some(2026-03-12T10:10:01),
    level: Info,
    message: "Executing CreateOrderController",
    request_id: Some("abc-123"),
    raw_line: <original>,
    line_number: 1,
}
```

**Testes:**
- Parsing de cada formato de timestamp suportado
- Parsing de cada formato de nível suportado
- Extração correta de request_id
- Linhas não reconhecidas retornam `LogEvent` com `level: Unknown` e mensagem = linha inteira
- Linhas vazias retornam `None`

**Fixture de teste** (`tests/fixtures/plain_logs.txt`):
```
2026-03-12 10:10:01 [INFO] RequestId=abc-123 Executing CreateOrderController
2026-03-12 10:10:02 [INFO] RequestId=abc-123 Executing GetUser
2026-03-12 10:10:03 [INFO] RequestId=abc-123 Executing GetCart
2026-03-12 10:10:04 [INFO] RequestId=abc-123 Executing CreateInvoice
2026-03-12 10:10:05 [ERROR] RequestId=abc-123 No provider found with name "paypau"
2026-03-12 10:10:06 [INFO] RequestId=def-456 Executing ListProductsController
2026-03-12 10:10:07 [INFO] RequestId=def-456 Executing GetProducts
2026-03-12 10:10:08 [INFO] RequestId=def-456 Completed successfully
```

**Arquivos criados:**
```
src/parser/mod.rs
src/parser/plain.rs
tests/fixtures/plain_logs.txt
```

---

### Step 4 — Classificador de Eventos

**Objetivo**: Analisar cada `LogEvent` e determinar se é entrada em função, erro ou log genérico.

**Tarefas:**

1. Criar `src/classifier/mod.rs` com função principal:
   ```rust
   pub fn classify(event: LogEvent) -> ClassifiedEvent;
   ```
2. Criar `src/classifier/patterns.rs` com regexes compiladas (lazy_static ou OnceCell):
   - **ENTRY patterns** — detectam entrada em função:
     - `Executing (?:method )?(\S+)`
     - `Enter(?:ing)? (\S+)`
     - `Starting (\S+)`
     - `Handling (\S+)`
     - `Processing (\S+)`
     - `Calling (\S+)`
     - `--> (\S+)`
   - **ERROR patterns** — detectam erros:
     - Nível do evento é `Error` ou `Fatal`
     - `(?:Exception|Error|PANIC|FATAL):\s*(.+)`
     - `throw (?:new )?(\w+)(?:\((.+)\))?`
     - `failed to .+:\s*(.+)`
     - `No \w+ found.+`
   - **EXIT patterns** — detectam saída de função:
     - `(\S+) completed`
     - `(\S+) finished`
     - `Exiting (\S+)`
     - `<-- (\S+)`
3. Criar `src/classifier/error.rs` com lógica para enriquecer `ErrorDetail`:
   - Extrair `ErrorType` (Throw se contém "throw", Exception se contém "Exception", etc.)
   - Extrair mensagem de erro limpa (sem prefixos de tipo)

**Critério de aceite:**
```
Input:  LogEvent { message: "Executing CreateOrderController", level: Info, ... }
Output: ClassifiedEvent { kind: Entry, function_name: Some("CreateOrderController"), ... }

Input:  LogEvent { message: "No provider found with name paypau", level: Error, ... }
Output: ClassifiedEvent { kind: Error, error_detail: Some(ErrorDetail { message: "No provider found...", type: Unknown }), ... }
```

**Testes:**
- Cada padrão de ENTRY é testado individualmente
- Cada padrão de ERROR é testado individualmente
- Evento com nível Error mas sem padrão → classificado como Error com mensagem original
- Evento INFO sem padrão → classificado como Log

**Arquivos criados:**
```
src/classifier/mod.rs
src/classifier/patterns.rs
src/classifier/error.rs
```

---

### Step 5 — Agrupador de Eventos por Requisição

**Objetivo**: Agrupar `ClassifiedEvent`s em conjuntos que pertencem à mesma requisição.

**Tarefas:**

1. Criar `src/grouper/mod.rs` com trait e orquestração:
   ```rust
   pub fn group_events(events: Vec<ClassifiedEvent>, config: &GroupConfig) -> Vec<RequestGroup>;

   pub struct RequestGroup {
       pub id: String,
       pub events: Vec<ClassifiedEvent>,
   }
   ```
2. Criar `src/grouper/by_id.rs` — agrupamento por `request_id` ou `trace_id`:
   - Coleta todos os eventos com o mesmo ID num `HashMap<String, Vec<ClassifiedEvent>>`
   - Eventos sem ID vão para um bucket "orphan"
3. Criar `src/grouper/by_time.rs` — agrupamento heurístico temporal:
   - Usado quando nenhum evento tem `request_id`
   - Itera eventos em ordem cronológica
   - Inicia novo grupo quando:
     - Gap entre evento atual e anterior excede `time_threshold_ms`
     - OU evento atual é um Entry e o grupo atual não tem Entry sem Exit correspondente
   - Gera IDs sintéticos para os grupos (`auto-1`, `auto-2`, etc.)
4. Lógica de seleção automática de estratégia:
   - Se >50% dos eventos têm `request_id` → usa `by_id`
   - Senão se >50% têm `thread_id` → usa `by_thread` (delegado a `by_id` usando thread_id como chave)
   - Senão → usa `by_time`

**Critério de aceite:**
```
Input: 8 eventos, 5 com request_id="abc-123", 3 com request_id="def-456"
Output: 2 grupos, um com 5 eventos e outro com 3

Input: 5 eventos sem request_id, gap de 2s entre evento 3 e 4 (threshold=500ms)
Output: 2 grupos, um com 3 eventos e outro com 2
```

**Testes:**
- Agrupamento por ID com múltiplos IDs
- Agrupamento temporal com gaps variados
- Seleção automática de estratégia
- Eventos sem timestamp no agrupamento temporal → ficam no grupo corrente

**Arquivos criados:**
```
src/grouper/mod.rs
src/grouper/by_id.rs
src/grouper/by_time.rs
```

---

### Step 6 — Construtor de Traces (Árvore de Execução)

**Objetivo**: Transformar cada `RequestGroup` numa árvore de `Span`s representando o fluxo de execução.

**Tarefas:**

1. Criar `src/trace_builder/mod.rs` com função principal:
   ```rust
   pub fn build_trace(group: RequestGroup) -> Trace;
   ```
2. Criar `src/trace_builder/stack.rs` — algoritmo de call stack:
   - Mantém um `Vec<Span>` como stack
   - Para cada evento do grupo (em ordem):
     - `Entry(name)` → cria novo Span, adiciona como filho do topo da stack, empilha
     - `Exit(name)` → se topo da stack tem mesmo nome, define `end_time` e desempilha
     - `Error(detail)` → atribui erro ao topo da stack
     - `Log` → adiciona como evento no topo da stack
   - Se a stack não está vazia ao final, desempilha tudo (spans sem exit explícito)
3. Implementar propagação de `has_error`:
   - Quando um Span recebe um erro, percorre todos os ancestrais marcando `has_error = true`
   - Implementar como pós-processamento: percorre a árvore bottom-up
4. Implementar cálculo de duração:
   - Se um Span tem `start_time` e `end_time`, calcula `duration`
   - Se tem `start_time` mas não `end_time`, usa o timestamp do último evento filho
   - Duration do trace = duration do root span
5. Implementar contadores no Trace:
   - `span_count`: contagem total de spans na árvore
   - `error_count`: contagem de spans com erro
   - `has_error`: true se `error_count > 0`

**Critério de aceite:**
```
Input (grupo com 5 eventos):
  Entry("CreateOrderController")  t=10:10:01
  Entry("GetUser")                t=10:10:02
  Entry("GetCart")                t=10:10:03
  Entry("CreateInvoice")          t=10:10:04
  Error("No provider found")      t=10:10:05

Output:
  Trace {
    root: Span("CreateOrderController") {
      has_error: true,
      children: [
        Span("GetUser") { has_error: false },
        Span("GetCart") { has_error: false },
        Span("CreateInvoice") {
          has_error: true,
          error: ErrorDetail("No provider found"),
        },
      ]
    },
    has_error: true,
    error_count: 1,
    span_count: 4,
  }
```

**Decisão de design**: Quando múltiplos Entry aparecem sequencialmente sem Exit entre eles, consideram-se **irmãos** (não aninhados), a menos que haja indício de hierarquia. Justificativa: em logs reais, a maioria dos métodos são logados apenas na entrada, sem log de saída. Tratar como irmãos sob o primeiro Entry (controller) é a heurística mais útil.

**Exceção**: Se um Entry é seguido imediatamente por outro Entry e depois por um Error, o Error é atribuído ao **último Entry antes dele**, não ao primeiro. Isso reflete que o erro ocorreu na função mais recente.

**Testes:**
- Eventos puramente sequenciais (sem Exit) → spans irmãos sob root
- Eventos com Entry/Exit pareados → spans aninhados corretamente
- Erro atribuído ao span correto
- Propagação de has_error até o root
- Cálculo de duração
- Grupo vazio → trace com root span vazio

**Arquivos criados:**
```
src/trace_builder/mod.rs
src/trace_builder/stack.rs
```

---

### Step 7 — Renderer em Árvore com Cores

**Objetivo**: Renderizar um `Trace` como árvore visual no terminal, com erros destacados.

**Tarefas:**

1. Criar `src/renderer/mod.rs` com trait:
   ```rust
   pub trait Renderer {
       fn render(&self, trace: &Trace, writer: &mut dyn Write) -> anyhow::Result<()>;
   }
   ```
2. Criar `src/renderer/colors.rs` com paleta de cores via crossterm:
   - `fn error_style()` → vermelho brilhante + bold
   - `fn success_style()` → verde
   - `fn dim_style()` → cinza para metadados
   - `fn warning_style()` → amarelo
   - `fn header_style()` → bold + underline
   - `fn duration_style()` → cinza claro
   - Struct `ColorConfig` com flag `enabled: bool` para suportar `--no-color`
3. Criar `src/renderer/tree.rs` com `TreeRenderer`:
   - **Header do trace**:
     ```
     Trace <id>  ─────────────────  <duration>  [❌ N errors | ✅ ok]
     ```
   - **Spans**: renderização recursiva com caracteres de árvore Unicode:
     - `├──` para irmãos intermediários
     - `└──` para último irmão
     - `│  ` para continuação vertical
     - `   ` para espaço sem continuação
   - **Spans com erro**: prefixo `❌` em vermelho
   - **Spans sem erro**: sem prefixo, cor padrão
   - **Duração**: alinhada à direita quando disponível, em cinza
   - **Erro inline**: abaixo do span com erro, indentado:
     ```
         └── ⚡ THROW: <mensagem>
     ```
   - **Error Summary**: bloco ao final do trace:
     ```
     ─── Error Summary ─────────────────
       ⚡ THROW │ <function> → "<message>"
     ────────────────────────────────────
     ```
4. Implementar cálculo de largura para alinhamento:
   - Duração alinhada à direita até largura do terminal (ou 80 como fallback)
   - Nomes de span truncados se excederem espaço disponível

**Critério de aceite:**
- Output visual corresponde ao exemplo mostrado no início deste documento
- Erros são visualmente impossíveis de ignorar (cor vermelha, símbolo ❌, summary)
- O path completo do erro é marcado do root até o span com erro
- `--no-color` produz output sem escape sequences ANSI
- Múltiplos traces são separados por linha em branco

**Testes:**
- Snapshot test (insta): trace sem erros → output esperado
- Snapshot test (insta): trace com 1 erro → output com propagação
- Snapshot test (insta): trace com múltiplos erros
- Teste de `--no-color`: output não contém escape sequences
- Teste de alinhamento de duração

**Arquivos criados:**
```
src/renderer/mod.rs
src/renderer/colors.rs
src/renderer/tree.rs
```

---

### Step 8 — Integração do Pipeline e main.rs

**Objetivo**: Conectar todos os módulos no `main.rs` para o fluxo completo funcionar.

**Tarefas:**

1. Atualizar `src/main.rs` com o pipeline completo:
   ```
   args = parse_cli()
   lines = read_input(args.files)        // Step 1
   events = parse_lines(lines)            // Step 3
   classified = classify_all(events)      // Step 4
   groups = group_events(classified)      // Step 5
   traces = build_traces(groups)          // Step 6
   render_all(traces)                     // Step 7
   ```
2. Implementar filtros no pipeline:
   - `--request <ID>`: após agrupamento, manter apenas o grupo com o ID correspondente
   - `--errors-only`: após construção de traces, manter apenas traces com `has_error == true`
   - `--last <N>`: após todos os filtros, manter apenas os últimos N traces
3. Implementar `src/lib.rs` com re-exports públicos de todos os módulos para facilitar testes de integração
4. Tratamento de erros amigável:
   - Arquivo não encontrado → mensagem clara com path
   - Sem permissão de leitura → mensagem clara
   - Nenhum trace encontrado → mensagem informativa (não é erro)
   - Request ID não encontrado → mensagem informativa listando IDs disponíveis

**Critério de aceite:**
```bash
# Pipeline completo funciona
flowtracer tests/fixtures/plain_logs.txt
# → mostra 2 traces (abc-123 e def-456)

# Filtro por request
flowtracer --request abc-123 tests/fixtures/plain_logs.txt
# → mostra apenas trace abc-123

# Filtro por erro
flowtracer --errors-only tests/fixtures/plain_logs.txt
# → mostra apenas trace abc-123 (que tem erro)

# Pipe funciona
cat tests/fixtures/plain_logs.txt | flowtracer
# → mesmo output que leitura direta

# Arquivo inexistente
flowtracer nope.txt
# → "Error: file not found: nope.txt"
```

**Arquivos modificados:**
```
src/main.rs  (reescrito)
src/lib.rs   (atualizado)
```

---

### Step 9 — Testes de Integração e Fixtures

**Objetivo**: Garantir que o pipeline end-to-end funciona com cenários realistas.

**Tarefas:**

1. Criar fixture `tests/fixtures/multi_request.txt`:
   - 3 requisições intercaladas (logs misturados)
   - 1 com erro, 2 sem erro
   - Cada uma com request_id diferente
2. Criar fixture `tests/fixtures/no_request_id.txt`:
   - Logs sem request_id
   - 2 "rajadas" de logs com gap temporal entre elas
   - Testa agrupamento por heurística temporal
3. Criar fixture `tests/fixtures/error_propagation.txt`:
   - Logs com chain de chamadas profunda (5+ níveis)
   - Erro no nível mais interno
   - Testa propagação visual de erro até o root
4. Criar `tests/integration_tests.rs`:
   - Teste: arquivo com múltiplas requisições → número correto de traces
   - Teste: `--request` filtra corretamente
   - Teste: `--errors-only` filtra corretamente
   - Teste: stdin pipe produz mesmo resultado que leitura de arquivo
   - Teste: logs sem request_id agrupa por tempo
   - Teste: erro propaga visualmente até root
5. Criar `tests/parser_tests.rs`:
   - Testes de cada formato de timestamp
   - Testes de extração de request_id
6. Criar `tests/classifier_tests.rs`:
   - Testes de cada padrão de Entry e Error

**Critério de aceite:**
- `cargo test` passa 100%
- Cobertura dos cenários principais: múltiplas requisições, erros, agrupamento temporal, filtros

**Arquivos criados:**
```
tests/fixtures/multi_request.txt
tests/fixtures/no_request_id.txt
tests/fixtures/error_propagation.txt
tests/integration_tests.rs
tests/parser_tests.rs
tests/classifier_tests.rs
```

---

### Step 10 — Polish e Performance (concluido)

**Objetivo**: Refinar a experiência, garantir performance e preparar para uso real.

**Tarefas:**

1. **Performance benchmark**:
   - Gerar fixture grande (100MB) com script
   - Medir tempo de processamento com `time flowtracer big.log`
   - Meta: <2 segundos para 100MB
   - Se necessário, otimizar: pre-compilar regexes com `once_cell::Lazy`, usar `memchr` para scanning rápido de linhas
2. **Mensagens de ajuda**:
   - `flowtracer --help` exibe exemplos de uso
   - Seção "EXAMPLES" no help com os 3 usos mais comuns
3. **Edge cases**:
   - Arquivo vazio → mensagem "No log entries found"
   - Arquivo binário → detectar e avisar
   - Linhas muito longas (>10KB) → truncar mensagem no output
   - Logs sem nenhum padrão reconhecido → mostrar em formato flat sem árvore, com aviso
4. **Output quando não há erros**:
   - Header do trace mostra `✅` em verde em vez de `❌`
   - Sem Error Summary

**Critério de aceite:**
- 100MB processado em <2s
- `--help` é claro e útil
- Nenhum panic em edge cases
- `cargo clippy` sem warnings
- `cargo fmt` aplicado

**Arquivos modificados:**
```
src/cli.rs       (help text melhorado)
src/renderer/tree.rs  (edge cases)
src/main.rs      (edge cases)
```

---

## Resumo dos Steps

| Step | Descrição | Dependências | Estimativa |
|---|---|---|---|
| 1 | Scaffold + CLI + Input | nenhuma | 2h |
| 2 | Modelo de dados | nenhuma | 1.5h |
| 3 | Parser plain text | Step 2 | 3h |
| 4 | Classificador de eventos | Steps 2, 3 | 2.5h |
| 5 | Agrupador por requisição | Steps 2, 4 | 2h |
| 6 | Construtor de traces | Steps 2, 5 | 3h |
| 7 | Renderer em árvore | Steps 2, 6 | 3h |
| 8 | Integração do pipeline | Steps 1–7 | 2h |
| 9 | Testes de integração | Step 8 | 2.5h |
| 10 | Polish e performance | Step 9 | 2h |
| | **Total estimado** | | **~23.5h** |

## Grafo de Dependências

```
Step 1 (CLI/Input) ──────────────────────────────────┐
                                                      │
Step 2 (Modelo) ─┬─ Step 3 (Parser) ─── Step 4 ──── Step 5 ──── Step 6 ──── Step 7
                 │                     (Classifier)  (Grouper)   (Builder)  (Renderer)
                 │                                                              │
                 └──────────────────────────────────────────────────────────────┤
                                                                                │
                                                              Step 8 (Integração)
                                                                    │
                                                              Step 9 (Testes E2E)
                                                                    │
                                                              Step 10 (Polish)
```

**Paralelização possível**: Steps 1 e 2 podem ser feitos em paralelo. Steps 3–7 são sequenciais pois cada um consome a saída do anterior.

---

## Definição de Pronto (DoD) da Fase 1

- [ ] `cargo build --release` compila sem warnings
- [ ] `cargo test` passa 100%
- [ ] `cargo clippy` sem warnings
- [ ] `flowtracer <arquivo>` exibe traces em árvore com erros destacados
- [ ] `cat <arquivo> | flowtracer` funciona via pipe
- [ ] `--request`, `--errors-only`, `--last`, `--no-color` funcionam
- [ ] Erros propagam visualmente do ponto de falha até o root span
- [ ] Error Summary exibido ao final de traces com erro
- [ ] 100MB de logs processado em <2 segundos
- [ ] README.md básico com instruções de instalação e uso
