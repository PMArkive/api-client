#!/bin/sh

docker rm -f api-test-db
docker rm -f api-test-fpm
docker rm -f api-test

mkdir -p /tmp/api-test-data
chmod 0777 /tmp/api-test-data

docker run -d --name api-test-db -e POSTGRES_PASSWORD=test -p 15432:5432 demostf/db
docker run -d --name api-test-fpm --link api-test-db:db -v /tmp/api-test-data:/demos \
  -e DEMO_ROOT=/demos -e DEMO_HOST=localhost -e DB_TYPE=pgsql \
  -e DB_HOST=db -e DB_PORT=5432 -e DB_DATABASE=postgres -e DB_USERNAME=postgres \
  -e DB_PASSWORD=test -e APP_ROOT=http://api.localhost -e EDIT_SECRET=edit \
  demostf/api
docker run -d --name api-test --link api-test-fpm:api -p 8888:80 demostf/api-nginx-test
