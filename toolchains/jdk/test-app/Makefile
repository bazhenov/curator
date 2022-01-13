.PHONY = run
run: .iid
	docker run --rm -it -l io.kubernetes.pod.name=java-test-app `cat .iid`

.iid:
	docker build --iidfile $@ . 

