# LogTrace: Reconstrução Visual de Fluxos de Execução a partir de Logs

## Visão Geral

**LogTrace** é uma ferramenta projetada para reconstruir e visualizar o fluxo de execução de requisições em aplicações backend utilizando **apenas logs existentes**, sem necessidade de instrumentação complexa ou adoção prévia de sistemas de tracing.

A ferramenta analisa logs verbosos de aplicações e reconstrói automaticamente:

- Fluxos de execução de uma requisição
- Cadeias de chamadas entre funções
- Origem de erros
- Fluxos assíncronos envolvendo mensageria e background jobs

O objetivo é transformar logs difíceis de interpretar em **traces claros e visuais**, facilitando debugging e entendimento do comportamento da aplicação.

---

# Problema

Em aplicações backend complexas:

- Logs são **muito verbosos**
- Informações relevantes ficam **misturadas com ruído**
- Seguir o fluxo de uma única requisição é **difícil**
- Sistemas distribuídos possuem **fluxos assíncronos**
- Erros aparecem sem contexto claro

Exemplo de logs comuns:


[INFO] Executing CreateOrderController
[INFO] Getting user
[INFO] Fetching cart
[INFO] Creating invoice
[ERROR] No provider found with name "paypau"


Com múltiplas requisições simultâneas, identificar **qual log pertence a qual fluxo** se torna complexo.

---

# Objetivo da Ferramenta

Transformar logs como:


[INFO] Executing CreateOrderController
[INFO] Fetching user
[INFO] Fetching cart
[INFO] Creating invoice
[ERROR] No provider found with name "paypau"


Em um trace claro:


createOrderController
→ GetUser
→ GetCart
→ CreateInvoice
→ GetProvider
❌ ERROR: No provider found with name "paypau"


Ou visualmente:


createOrderController
└─ GetUser
└─ GetCart
└─ CreateInvoice
└─ GetProvider
└─ ERROR: No provider found with name "paypau"


---

# Ideia Central

A ferramenta reconstrói **traces de execução a partir de logs** utilizando:

1. Parsing de logs
2. Agrupamento de eventos por requisição
3. Extração de chamadas de função
4. Reconstrução do fluxo
5. Renderização visual

Pipeline:


Logs brutos
↓
Parser de logs
↓
Extração de eventos
↓
Agrupamento por requisição
↓
Reconstrução de fluxo
↓
Visualização do trace


---

# Entrada da Ferramenta

Logs provenientes de:

- stdout do servidor
- arquivos de log
- pipelines de observabilidade
- containers
- sistemas de logging

Exemplo:


2026-03-12 10:10:01 Enter CreateOrderController
2026-03-12 10:10:02 Enter GetUser
2026-03-12 10:10:03 Enter GetCart
2026-03-12 10:10:04 Enter CreateInvoice
2026-03-12 10:10:05 ERROR No provider found with name paypau


---

# Identificação de Requisições

A ferramenta pode agrupar logs por requisição utilizando:

## 1. RequestId (quando disponível)

Logs contendo identificador de requisição:


RequestId=abc123 CreateOrderController
RequestId=abc123 GetUser
RequestId=abc123 GetCart


Agrupamento:


group logs by RequestId


---

## 2. TraceId (padrão de observabilidade)

Utilização de traceId propagado entre serviços.


TraceId=4bf92f3577b34da6a3ce929d0e0e4736


Permite reconstruir **fluxos distribuídos entre serviços**.

---

## 3. ThreadId

Se logs contiverem threadId:


[Thread 12] CreateOrderController
[Thread 12] GetUser
[Thread 12] ERROR


---

## 4. Heurísticas de Timestamp

Caso não exista identificador explícito:

Agrupar logs por proximidade temporal.

Exemplo:


threshold = 500ms


Logs próximos no tempo são considerados parte da mesma requisição.

---

# Extração de Fluxo de Execução

A ferramenta identifica chamadas de função utilizando padrões de log.

Exemplo:


Executing method PaymentService.GetProvider


Regex possível:


Executing method (\w+.\w+)


Resultado:


PaymentService.GetProvider


---

# Reconstrução usando StackTrace

Quando ocorre uma exceção, o stacktrace contém a cadeia completa de chamadas.

Exemplo:


System.Exception: No provider found
at PaymentService.GetProvider
at InvoiceService.CreateInvoice
at CartService.GetCart
at UserService.GetUser
at OrderController.CreateOrder


A ferramenta pode extrair automaticamente:


CreateOrder
→ GetUser
→ GetCart
→ CreateInvoice
→ GetProvider


Isso permite reconstruir o fluxo **mesmo sem logs intermediários**.

---

# Reconstrução de Call Graph da Aplicação

Ao analisar múltiplos stacktraces ao longo do tempo, a ferramenta pode construir um **grafo global de chamadas da aplicação**.

Exemplo de edges detectadas:


CreateOrderController → GetUser
GetUser → GetCart
GetCart → CreateInvoice
CreateInvoice → GetProvider


Grafo resultante:


CreateOrderController
|
v
GetUser
|
v
GetCart
|
v
CreateInvoice
|
v
GetProvider


Isso revela:

- arquitetura real do sistema
- caminhos críticos
- dependências entre serviços

---

# Suporte a Fluxos Assíncronos

Em sistemas com pub/sub ou background jobs, o contexto pode ser propagado via **metadata da mensagem**.

Exemplo de evento:


{
"orderId": 123,
"traceId": "abc123"
}


Fluxo:


HTTP Request
↓
CreateOrderController
↓
Publish Event
↓
Message Broker
↓
Background Worker
↓
ProcessOrder


Logs com mesmo traceId podem ser correlacionados.

---

# Fluxo Distribuído Reconstruído

Exemplo:


HTTP POST /orders
│
├── CreateOrderController
│
├── Publish OrderCreated
│
▼
Message Broker
│
▼
OrderWorker
│
├── ProcessOrder
└── SendEmail


Tudo reconstruído a partir de logs.

---

# Visualizações Possíveis

## Linear Trace


CreateOrderController
→ GetUser
→ GetCart
→ CreateInvoice
→ GetProvider
❌ No provider found


---

## Árvore de Execução


CreateOrderController
├─ GetUser
├─ GetCart
└─ CreateInvoice
└─ GetProvider
└─ ERROR


---

## Call Graph


Controller → Service → Repository → ExternalProvider


---

## Sequence Diagram


Client → API → OrderService → PaymentService → EmailWorker


---

# Casos de Uso

## Debugging de Produção

Seguir exatamente o caminho que levou a um erro.

---

## Análise de Fluxo

Entender como a aplicação realmente executa internamente.

---

## Observabilidade sem Instrumentação

Criar tracing mesmo quando o sistema não foi projetado para isso.

---

## Análise de Arquitetura

Descobrir dependências reais entre componentes.

---

# Possíveis Formas de Uso

CLI:


flowtrace --request abc123 logs.txt


Ou streaming:


server | flowtrace


---

# Arquitetura da Ferramenta


logs stream
↓
log parser
↓
event extractor
↓
request reconstruction
↓
trace builder
↓
visual renderer


Componentes principais:

- Parser de logs
- Normalizador de eventos
- Reconstrutor de requisições
- Builder de traces
- Motor de visualização

---

# Diferencial da Ideia

A maioria das ferramentas de tracing exige:

- instrumentação manual
- SDKs específicos
- alteração no código

LogTrace propõe:

**Distributed tracing reconstruído apenas a partir de logs.**

Sem modificar a aplicação.

---

# Possíveis Extensões Futuras

- geração automática de diagramas
- análise de latência entre chamadas
- identificação de gargalos
- detecção de padrões de erro
- análise de dependências entre serviços
- reconstrução automática de arquitetura

---

# Resumo

LogTrace transforma logs brutos em **traces compreensíveis**, permitindo:

- seguir o fluxo completo de uma requisição
- identificar rapidamente onde um erro ocorreu
- reconstruir fluxos distribuídos
- entender a arquitetura real da aplicação

Tudo isso **sem instrumentação adicional**, utilizando apenas os logs já existentes.
