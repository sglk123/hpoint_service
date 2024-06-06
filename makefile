
IMAGE_NAME := hpoint_serivce
IMAGE_TAG := latest

CONTAINER_NAME := sglk_hpoint

build:
	docker build -t $(IMAGE_NAME):$(IMAGE_TAG) .

run:
	docker run -d --name $(CONTAINER_NAME) $(IMAGE_NAME):$(IMAGE_TAG)

stop:
	docker stop $(CONTAINER_NAME)
	docker rm $(CONTAINER_NAME)

clean:
	docker rmi $(IMAGE_NAME):$(IMAGE_TAG)

run_binary_websocket:
	cargo run --manifest-path network_websocket/Cargo.toml -- --config ./network_websocket/config.yaml

run_binary_grpc:
	cargo run --manifest-path network_grpc/Cargo.toml -- --config ./network_grpc/config.yaml

run_pg:
	sudo apt update
	sudo apt install postgresql postgresql-contrib
	sudo systemctl start postgresql
	sudo systemctl enable postgresql
	#   sudo -i -u postgres
    #   psql
    #   CREATE USER sglk WITH PASSWORD '123';
    #   CREATE DATABASE mydatabase OWNER sglk;
    #   \q
    #   sudo nano /etc/postgresql/{version}/main/postgresql.conf
    #   listen_addresses = '*' / 0.0.0.0
    #   sudo nano /etc/postgresql/12/main/pg_hba.conf
    #   host    all             all             0.0.0.0/0               md5
    #   sudo systemctl restart postgresql




