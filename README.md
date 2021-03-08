## Building images

```
$ make images
```

## Running integration tests

```
$ docker-compose run it-tests
$ docker-compose down -t 0
```

## Running the system

```
$ docker-compose up
```

## Making toolchain images

```
$ make toolchains/[name]
```

## Running toolchain

```
$ docker run -it bazhenov.me/curator/toolchain-[name]:dev
```