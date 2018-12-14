FROM build-template

ARG TAG=master

RUN git clone https://github.com/kiibohd/controller.git -b $TAG
RUN cd controller/Keyboards; pipenv install

WORKDIR /controller/Keyboards
