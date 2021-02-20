Building images:

```
make images
```

Running the system (server, test agent and frontend):

```
docker-compose up
```

Making toolchain images:

```
make toolchains/[name]
```

Running toolchain:

```
docker run -it bazhenov.me/curator/toolchain-[name]:dev
```