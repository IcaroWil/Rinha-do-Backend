# Rinha de Backend 2026 - Fraud Detection API

API desenvolvida para o desafio **Rinha de Backend 2026**, com foco em **detecção de fraude em transações de cartão usando busca vetorial**.

A solução implementa um módulo de autorização antifraude que recebe uma transação, transforma o payload em um vetor de 14 dimensões, busca transações similares em um dataset de referência e retorna uma decisão de aprovação ou negação com base no score de fraude.

---

## Objetivo

Para cada transação recebida em `POST /fraud-score`, a aplicação executa o seguinte fluxo:

1. Recebe o payload da transação.
2. Normaliza os campos conforme as regras oficiais do desafio.
3. Converte a transação em um vetor de 14 dimensões.
4. Busca os 5 vetores mais próximos no dataset de referência.
5. Calcula o score de fraude com base nos rótulos encontrados.
6. Retorna a decisão final.

A regra de decisão é:

```txt
fraud_score = quantidade_de_fraudes_entre_os_5_vizinhos / 5
approved = fraud_score < 0.6
```

---

## Stack

A stack foi escolhida com foco em baixa latência, baixo consumo de memória e previsibilidade de performance.

- **Rust** — linguagem principal da API.
- **Axum** — framework HTTP minimalista e performático.
- **Tokio** — runtime assíncrono.
- **Serde** — serialização e desserialização JSON.
- **Nginx** — load balancer com round-robin.
- **Docker Compose** — orquestração local conforme exigência do desafio.
- **Índice binário próprio** — pré-processamento do dataset para reduzir custo de startup e parsing.

---

## Arquitetura

A solução segue a arquitetura mínima exigida pelo desafio:

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

O load balancer apenas distribui as requisições entre as instâncias da API.  
Toda a lógica de detecção de fraude fica exclusivamente dentro das APIs.

---

## Estrutura do projeto

```txt
.
├── src/
│   ├── main.rs              # Inicialização da aplicação
│   ├── api.rs               # Rotas HTTP e handlers
│   ├── models.rs            # Contratos de request/response
│   ├── vectorizer.rs        # Conversão do payload em vetor de 14 dimensões
│   ├── search.rs            # Algoritmo de busca vetorial
│   ├── dataset.rs           # Carregamento do índice binário
│   ├── config.rs            # Carregamento de normalização e risco MCC
│   └── bin/
│       └── build_index.rs   # Gerador do índice binário
├── data/
│   └── .gitkeep
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

Endpoint de healthcheck/readiness.

#### Response

```http
HTTP/1.1 200 OK
```

---

### `POST /fraud-score`

Recebe os dados de uma transação e retorna a decisão antifraude.

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

As dimensões são normalizadas para permitir comparação por distância vetorial.

Quando `last_transaction` é `null`, as dimensões relacionadas à última transação recebem o valor especial `-1`.

---

## Estratégia de busca vetorial

A primeira versão da solução utiliza busca exata por distância euclidiana sobre o índice binário carregado em memória.

Para cada transação:

1. O payload é convertido para vetor.
2. O vetor é quantizado.
3. A aplicação percorre os vetores de referência.
4. Mantém apenas os 5 vizinhos mais próximos.
5. Calcula o score final com base nos labels dos vizinhos.

A distância é calculada sem raiz quadrada, pois para comparação de vizinhos a soma dos quadrados é suficiente.

```txt
distance = Σ(query[i] - reference[i])²
```

---

## Pré-processamento do dataset

O dataset oficial é fornecido como JSON gzipado.

Para evitar parsing de JSON grande durante o startup da API, o projeto possui um binário auxiliar responsável por gerar um índice binário otimizado.

Fluxo:

```txt
data/references.json.gz
        |
        v
cargo run --release --bin build_index
        |
        v
data/index.bin
```

O arquivo `index.bin` contém:

```txt
Header
Quantidade de vetores
Quantidade de dimensões
Vetores quantizados em u16
Labels em u8
```

Essa abordagem reduz o custo de leitura em runtime e permite que a API carregue diretamente uma representação compacta do dataset.

---

## Dados necessários

Os arquivos oficiais do desafio devem estar dentro da pasta `data/`.

Baixe os arquivos com:

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

Valide o arquivo compactado:

```bash
gzip -t data/references.json.gz
```

Se o comando não retornar erro, o arquivo está válido.

---

## Gerar índice binário

Após baixar os arquivos, gere o índice:

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

Execute a API em modo release:

```bash
cargo run --release --bin rinha-fraud-rust
```

A aplicação deve iniciar em:

```txt
0.0.0.0:8080
```

Teste o readiness:

```bash
curl -i http://localhost:8080/ready
```

Teste o endpoint principal:

```bash
jq '.[0]' data/example-payloads.json > /tmp/payload.json

curl -s -X POST http://localhost:8080/fraud-score \
  -H "Content-Type: application/json" \
  --data @/tmp/payload.json
```

---

## Rodar com Docker

Build da imagem:

```bash
docker build -t rinha-fraud-rust:latest .
```

Executar apenas uma instância:

```bash
docker run --rm -p 8080:8080 rinha-fraud-rust:latest
```

Teste:

```bash
curl -i http://localhost:8080/ready
```

---

## Rodar com Docker Compose

A execução via Docker Compose sobe:

- 1 Nginx na porta `9999`
- 2 instâncias da API Rust

```bash
docker compose up
```

Teste pela porta oficial:

```bash
curl -i http://localhost:9999/ready
```

Teste o endpoint principal:

```bash
curl -s -X POST http://localhost:9999/fraud-score \
  -H "Content-Type: application/json" \
  --data @/tmp/payload.json
```

---

## Limites de recursos

O `docker-compose.yml` define limites compatíveis com o desafio:

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

---

## Benchmark simples

Para medir o tempo de uma requisição:

```bash
time curl -s -X POST http://localhost:9999/fraud-score \
  -H "Content-Type: application/json" \
  --data @/tmp/payload.json
```

Para acompanhar consumo de memória e CPU:

```bash
docker stats
```

---

## Arquivos versionados

Este repositório versiona:

```txt
src/
Cargo.toml
Cargo.lock
Dockerfile
docker-compose.yml
nginx.conf
README.md
.gitignore
.dockerignore
data/.gitkeep
```

---

## Arquivos não versionados

Por serem arquivos grandes, gerados ou específicos do ambiente local, estes arquivos não são versionados:

```txt
target/
data/references.json.gz
data/index.bin
data/*.bin
.env
*.log
```

O índice binário deve ser gerado localmente antes do build da imagem Docker.

---

## Comandos úteis

### Build em release

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

### Ver containers ativos

```bash
docker ps
```

### Ver consumo de recursos

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
- [x] Dockerfile.
- [x] Docker Compose com Nginx e 2 APIs.
- [ ] Otimização de busca vetorial.
- [ ] Redução de latência p99.
- [ ] Validação de consumo dentro dos limites finais.
- [ ] Preparação da branch `submission`.

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

O dataset oficial é grande e não deve ser parseado como JSON em cada startup de forma custosa.

O índice binário permite:

- Menor uso de espaço.
- Startup mais rápido.
- Leitura sequencial eficiente.
- Representação compacta dos vetores.
- Melhor controle sobre layout de memória.

### Por que Nginx?

O Nginx é utilizado apenas como load balancer, respeitando a regra do desafio de que o balanceador não pode aplicar lógica de detecção.

---

## Status atual

A solução já possui uma implementação funcional com busca vetorial exata sobre o dataset real pré-processado.

Próximos esforços serão concentrados em otimização de latência e redução de custo de busca por request.