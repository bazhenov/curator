# syntax = docker/dockerfile:experimental
FROM node:14.0.0-stretch AS build

WORKDIR /root
COPY package.json ./
COPY package-lock.json ./

RUN --mount=type=cache,target=./node_modules npm install

COPY src ./src
COPY public ./public
COPY tsconfig.json ./
RUN --mount=type=cache,target=./node_modules npm run build

FROM nginx:stable AS frontend

COPY --from=build /root/build /var/www
COPY ./docker/nginx-frontend.conf /etc/nginx/conf.d/default.conf

EXPOSE 8080
ENTRYPOINT  ["nginx", "-g", "daemon off;"]

FROM nginx:stable AS router

COPY ./docker/nginx-router.conf /etc/nginx/conf.d/default.conf

EXPOSE 8080
ENTRYPOINT  ["nginx", "-g", "daemon off;"]