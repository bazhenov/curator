## Building images

```
$ make images
```

## Running integration tests

```
$ make run-tests
```

## Running the system

```
$ make run
```

## Making toolchain images

```
$ make toolchains/[name] -B
```

## Running toolchain

```
$ docker run -it --pid=container:$CONTAINER_PID -it bazhenov.me/curator/toolchain-[name]:dev /discover
```