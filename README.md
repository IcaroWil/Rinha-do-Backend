# Rinha de Backend 2026 - Fraud Detection API

API desenvolvida para o desafio **Rinha de Backend 2026**, com foco em **detecção de fraude em transações de cartão usando busca vetorial**.

A solução implementa um módulo antifraude que recebe uma transação de cartão, transforma o payload em um vetor de 14 dimensões, busca transações similares em um dataset de referência e retorna uma decisão de aprovação ou negação com base no score de fraude.

---

## Status da submissão

Repositório submetido no desafio:

```txt
https://github.com/IcaroWil/Rinha-do-Backend
```

Imagem pública Docker:

```txt
wilcaro572/rinha-fraud-rust:latest
```

Branches:

```txt
main        -> código-fonte completo
submission  -> arquivos mínimos para execução oficial
```

A branch `submission` contém apenas:

```txt
docker-compose.yml
nginx.conf
```

---

## Objetivo

Para cada transação recebida em `POST /fraud-score`, a aplicação executa o seguinte fluxo:

1. Recebe o payload da transação.
2. Normaliza os campos conforme as regras oficiais do desafio.
3. Converte a transação em um vetor de 14 dimensões.
4. Busca os 5 vetores mais próximos no dataset de referência.
5. Calcula o score de fraude com base nos rótulos encontrados.
6. Retorna a decisão final.

Regra de decisão:

```txt
fraud_score = quantidade_de_fraudes_entre_os_5_vizinhos / 5
approved = fraud_score < 0.6
```

---

## Stack

A stack foi escolhida com foco em baixa latência, previsibilidade de performance e baixo consumo de memória.

- **Rust** — linguagem principal da API.
- **Axum** — framework HTTP.
- **Tokio** — runtime assíncrono.
- **Serde** — serialização e desserialização JSON.
- **Nginx** — load balancer round-robin.
- **Docker Compose** — orquestração local e submissão.
- **Índice binário próprio** — pré-processamento do dataset para reduzir parsing em runtime.
- **Busca vetorial bucketizada** — otimização para reduzir candidatos por request.

---

## Arquitetura

A solução segue a arquitetura exigida pelo desafio:

```txt
Client
  |
  v
Nginx Load Balancer :9999
  |
  +--> API 1 :8080
  |
  +--> API 2 :8080
```

O load balancer apenas distribui requisições entre as instâncias da API.

Toda a lógica de detecção de fraude fica dentro das APIs Rust.

---

## Estrutura do projeto

```txt
.
├── src/
│   ├── main.rs                 # Inicialização da aplicação
│   ├── lib.rs                  # Exposição dos módulos para binários auxiliares
│   ├── api.rs                  # Rotas HTTP e handlers
│   ├── models.rs               # Contratos de request/response
│   ├── vectorizer.rs           # Conversão do payload em vetor de 14 dimensões
│   ├── search.rs               # Busca vetorial otimizada
│   ├── dataset.rs              # Carregamento do índice binário e buckets
│   ├── config.rs               # Normalização e risco MCC
│   └── bin/
│       ├── build_index.rs      # Gerador do índice binário
│       ├── compare_search.rs   # Comparador bucket vs full scan
│       ├── bench_search.rs     # Benchmark interno da busca
│       └── bench_by_payload.rs # Benchmark por payload
├── data/
│   └── .gitkeep
├── scripts/
│   └── load_test_varied.sh
├── Dockerfile
├── docker-compose.yml
├── nginx.conf
├── Cargo.toml
├── Cargo.lock
├── .gitignore
├── .dockerignore
└── README.md
```

---

## Endpoints

### `GET /ready`

Endpoint de readiness.

#### Response

```http
HTTP/1.1 200 OK
```

---

### `POST /fraud-score`

Recebe os dados da transação e retorna a decisão antifraude.

#### Request example

```json
{
  "id": "tx-123",
  "transaction": {
    "amount": 384.88,
    "installments": 3,
    "requested_at": "2026-01-01T12:00:00Z"
  },
  "customer": {
    "avg_amount": 769.76,
    "tx_count_24h": 3,
    "known_merchants": ["MERC-002"]
  },
  "merchant": {
    "id": "MERC-001",
    "mcc": "5912",
    "avg_amount": 298.95
  },
  "terminal": {
    "is_online": false,
    "card_present": true,
    "km_from_home": 13.7
  },
  "last_transaction": {
    "timestamp": "2026-01-01T11:20:00Z",
    "km_from_current": 18.8
  }
}
```

#### Response example

```json
{
  "approved": false,
  "fraud_score": 0.8
}
```

---

## Vetorização

Cada transação é convertida em um vetor de 14 dimensões:

```txt
0  amount
1  installments
2  amount_vs_avg
3  hour_of_day
4  day_of_week
5  minutes_since_last_tx
6  km_from_last_tx
7  km_from_home
8  tx_count_24h
9  is_online
10 card_present
11 unknown_merchant
12 mcc_risk
13 merchant_avg_amount
```

As dimensões são normalizadas conforme as regras oficiais do desafio.

Quando `last_transaction` é `null`, as dimensões relacionadas à última transação recebem tratamento especial conforme a regra de normalização.

---

## Estratégia de busca vetorial

A solução começou com uma busca exata por distância euclidiana sobre os 3 milhões de vetores.

Depois, foi otimizada para uma busca bucketizada:

1. O payload é convertido para vetor normalizado.
2. O vetor é quantizado para `u16`.
3. A consulta identifica buckets candidatos com base em dimensões relevantes.
4. A distância é calculada apenas nos candidatos do bucket.
5. Os 5 vizinhos mais próximos são mantidos.
6. O `fraud_score` é calculado com base nos labels dos 5 vizinhos.

A distância usa soma dos quadrados, sem raiz quadrada:

```txt
distance = Σ(query[i] - reference[i])²
```

A raiz quadrada não é necessária porque a ordenação das distâncias não muda.

---

## Otimizações aplicadas

### Índice binário

O dataset oficial vem como JSON gzipado.

Para evitar parsing custoso em runtime, o projeto gera um índice binário:

```txt
references.json.gz
        |
        v
build_index
        |
        v
index.bin
```

O índice armazena:

```txt
magic header
quantidade de vetores
quantidade de dimensões
vetores quantizados em u16
labels em u8
```

### Bucket search

A busca foi otimizada usando buckets calculados a partir de dimensões fortes do vetor.

A estratégia final usa:

```txt
AMOUNT_BUCKETS = 16
MCC_BUCKETS = 8
```

Busca principal:

```txt
amount exato
mcc ±1
```

Para scores de borda:

```txt
score 0.4 ou 0.6
```

a solução faz uma expansão local:

```txt
amount ±1
mcc ±1
```

Isso evita fallback para full scan na maioria dos casos de fronteira, mantendo boa compatibilidade com a busca exata nos payloads de exemplo.

### Distância desenrolada

Como o vetor sempre possui 14 dimensões, o cálculo da distância foi desenrolado manualmente para reduzir overhead no caminho quente.

---

## Pré-processamento do dataset

Baixe os arquivos oficiais:

```bash
mkdir -p data

curl -L \
  https://raw.githubusercontent.com/zanfranceschi/rinha-de-backend-2026/main/resources/mcc_risk.json \
  -o data/mcc_risk.json

curl -L \
  https://raw.githubusercontent.com/zanfranceschi/rinha-de-backend-2026/main/resources/normalization.json \
  -o data/normalization.json

curl -L \
  https://raw.githubusercontent.com/zanfranceschi/rinha-de-backend-2026/main/resources/example-references.json \
  -o data/example-references.json

curl -L \
  https://raw.githubusercontent.com/zanfranceschi/rinha-de-backend-2026/main/resources/example-payloads.json \
  -o data/example-payloads.json

curl -L \
  https://github.com/zanfranceschi/rinha-de-backend-2026/raw/main/resources/references.json.gz \
  -o data/references.json.gz
```

Valide o arquivo:

```bash
gzip -t data/references.json.gz
```

Gere o índice:

```bash
cargo run --release --bin build_index
```

Saída esperada:

```txt
Reading "data/references.json.gz"
Loaded 3000000 references
Index written to "data/index.bin"
Index size: 82.97 MB
```

---

## Rodar localmente

```bash
cargo run --release --bin rinha-fraud-rust
```

Teste:

```bash
curl -i http://localhost:8080/ready
```

```bash
jq '.[0]' data/example-payloads.json > /tmp/payload.json

curl -s -X POST http://localhost:8080/fraud-score \
  -H "Content-Type: application/json" \
  --data @/tmp/payload.json
```

---

## Rodar com Docker

Build local:

```bash
docker build -t rinha-fraud-rust:latest .
```

Rodar uma instância:

```bash
docker run --rm -p 8080:8080 rinha-fraud-rust:latest
```

Teste:

```bash
curl -i http://localhost:8080/ready
```

---

## Rodar com Docker Compose

A execução com Docker Compose sobe:

- 1 Nginx na porta `9999`
- 2 instâncias da API Rust

```bash
docker compose up
```

Teste:

```bash
curl -i http://localhost:9999/ready
```

```bash
curl -s -X POST http://localhost:9999/fraud-score \
  -H "Content-Type: application/json" \
  --data @/tmp/payload.json
```

---

## Docker Hub

Imagem pública usada na branch `submission`:

```txt
wilcaro572/rinha-fraud-rust:latest
```

Pull manual:

```bash
docker pull wilcaro572/rinha-fraud-rust:latest
```

---

## Limites de recursos

O `docker-compose.yml` respeita o limite total do desafio:

```txt
nginx:
  cpu: 0.05
  memory: 16MB

api1:
  cpu: 0.475
  memory: 167MB

api2:
  cpu: 0.475
  memory: 167MB
```

Total:

```txt
CPU: 1.0
Memory: 350MB
```

Durante os testes locais, o consumo ficou aproximadamente:

```txt
nginx: ~3 MB
api1:  ~96 MB
api2:  ~96 MB
```

---

## Benchmarks

### Comparar busca bucketizada contra busca exata

```bash
cargo run --release --bin compare_search
```

Resultado validado nos payloads de exemplo:

```txt
Same score: 50/50
Same decision: 50/50
```

### Benchmark interno da busca

```bash
cargo run --release --bin bench_search
```

Resultado observado após otimizações:

```txt
avg: ~7.3ms
p99: ~16.9ms
```

### Benchmark por payload

```bash
cargo run --release --bin bench_by_payload
```

Esse comando mostra quais payloads caem em buckets mais caros e quantos candidatos são avaliados.

### Teste de carga com payloads variados

```bash
./scripts/load_test_varied.sh http://localhost:9999/fraud-score 500 4
```

Resultado observado:

```txt
Average: ~30ms
p99: ~92ms
Status codes:
200 500
```

```bash
./scripts/load_test_varied.sh http://localhost:9999/fraud-score 1000 10
```

Resultado observado:

```txt
Average: ~95ms
p99: ~269ms
Status codes:
200 1000
```

---

## Arquivos versionados

Este repositório versiona:

```txt
src/
scripts/
Cargo.toml
Cargo.lock
Dockerfile
docker-compose.yml
nginx.conf
README.md
.gitignore
.dockerignore
data/.gitkeep
data/example-payloads.json
data/example-references.json
data/mcc_risk.json
data/normalization.json
```

---

## Arquivos não versionados

```txt
target/
data/references.json.gz
data/index.bin
data/*.bin
.env
*.log
```

---

## Comandos úteis

### Build release

```bash
cargo build --release
```

### Rodar API

```bash
cargo run --release --bin rinha-fraud-rust
```

### Gerar índice

```bash
cargo run --release --bin build_index
```

### Comparar busca bucketizada e full scan

```bash
cargo run --release --bin compare_search
```

### Benchmark interno

```bash
cargo run --release --bin bench_search
```

### Benchmark por payload

```bash
cargo run --release --bin bench_by_payload
```

### Build Docker

```bash
docker build -t rinha-fraud-rust:latest .
```

### Subir ambiente completo

```bash
docker compose up
```

### Derrubar ambiente

```bash
docker compose down
```

### Ver consumo

```bash
docker stats
```

---

## Roadmap técnico

- [x] API HTTP com Rust e Axum.
- [x] Endpoint `GET /ready`.
- [x] Endpoint `POST /fraud-score`.
- [x] Vetorização com 14 dimensões.
- [x] Leitura de arquivos de normalização e MCC risk.
- [x] Geração de índice binário.
- [x] Carregamento do índice em memória.
- [x] Busca vetorial exata inicial.
- [x] Otimização com bucket search.
- [x] Expansão local para scores de borda.
- [x] Cálculo de distância otimizado.
- [x] Dockerfile.
- [x] Docker Compose com Nginx e 2 APIs.
- [x] Imagem pública no Docker Hub.
- [x] Validação de consumo dentro dos limites finais.
- [x] Benchmarks internos.
- [x] Teste de carga com payloads variados.
- [x] Preparação da branch `submission`.
- [x] PR de participação no repositório oficial.

---

## Decisões técnicas

### Por que Rust?

Rust foi escolhido por oferecer:

- Baixa latência.
- Controle fino de memória.
- Ausência de garbage collector.
- Boa performance em processamento intensivo.
- Binários pequenos e previsíveis.
- Segurança de memória em tempo de compilação.

### Por que índice binário?

O dataset oficial é grande e não deve ser parseado como JSON no startup da API.

O índice binário permite:

- Carregamento mais previsível.
- Representação compacta.
- Vetores quantizados em `u16`.
- Menor overhead de parsing.
- Melhor controle do layout de memória.

### Por que bucket search?

A busca exata nos 3 milhões de vetores é correta, mas custosa.

A bucketização reduz a quantidade de candidatos avaliados por request e melhora a latência, mantendo o mesmo resultado da busca exata nos payloads de exemplo.

### Por que Nginx?

O Nginx é utilizado apenas como load balancer round-robin, respeitando a regra do desafio de que o balanceador não pode aplicar lógica de detecção.

Também foi testado HAProxy, mas o Nginx apresentou melhor estabilidade e menor consumo para esta solução.

---

## Status atual

A solução está funcional, otimizada e submetida ao repositório oficial da Rinha de Backend 2026.

O repositório possui:

```txt
main        -> código completo
submission  -> arquivos mínimos para execução oficial
```

A imagem pública está disponível em:

```txt
wilcaro572/rinha-fraud-rust:latest
```