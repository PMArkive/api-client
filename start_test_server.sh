#!/bin/sh

docker rm -f api-test-db
docker rm -f api-test-fpm
docker rm -f api-test

docker run -d --name api-test-db -e POSTGRES_PASSWORD=test demostf/db
docker run -d --name api-test-fpm --link api-test-db:db -v /demos \
  -e DEMO_ROOT=/demos -e DEMO_HOST=localhost -e DB_TYPE=pgsql \
  -e DB_HOST=db -e DB_PORT=5432 -e DB_DATABASE=postgres -e DB_USERNAME=postgres \
  -e DB_PASSWORD=test -e APP_ROOT=https://localhost -e EDIT_SECRET=edit \
  demostf/api
docker run -d --name api-test --link api-test-fpm:api -e HOST=localhost -e UPLOAD_FASTCGI=api:9000 \
     -e UPLOAD_SCRIPT=/app/src/public/upload.php -p 8888:80 demostf/demos.tf

sleep 2

# instead of having to deal with mocking steam login we just manually create our first user
docker exec -u postgres api-test-db psql -c "INSERT INTO users(steamid, name, avatar, token)
  VALUES(76561198024494988, 'Icewind', 'http://cdn.akamai.steamstatic.com/steamcommunity/public/images/avatars/75/75b84075b70535c5cfb3499af03b3e4e7a7b556f_medium.jpg', 'test_token')"