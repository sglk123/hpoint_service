# USER GUIDE
1. RUN SERVER <br>
make run_binary_websocket <br>

2. configuration <br>
./websocket/config.yaml

```yaml
port: 127.0.0.1:8888        // websocket port
wport: 0.0.0.0:8889         // restful http port
db:
  host: localhost
  port: 5432
  user: postgres
  password: 123
  dbname: sglk_sample1
