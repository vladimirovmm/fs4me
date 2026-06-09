FROM ubuntu:latest

# Устанавливаем SSH‑сервер
RUN apt-get update && \
    apt-get install -y openssh-server && \
    rm -rf /var/lib/apt/lists/*

# Создаём директорию для SSH
RUN mkdir -p /var/run/sshd

# Настраиваем SSH
RUN sed -i 's/#PermitRootLogin prohibit-password/PermitRootLogin no/' /etc/ssh/sshd_config && \
    sed -i 's/#PasswordAuthentication yes/PasswordAuthentication yes/' /etc/ssh/sshd_config && \
    sed -i 's/#PubkeyAuthentication yes/PubkeyAuthentication yes/' /etc/ssh/sshd_config


# Выполняем настройки: создание пользователя, установка пароля, добавление SSH‑ключа
RUN useradd -m -s /bin/bash {{USER}} && \
    echo '{{USER}}:{{PASSWORD}}' | chpasswd && \
    mkdir -p /home/{{USER}}/.ssh && \
    echo '{{PUB_KEY}}' > /home/{{USER}}/.ssh/authorized_keys && \
    chown -R {{USER}}:{{USER}} /home/{{USER}}/.ssh && \
    chmod 700 /home/{{USER}}/.ssh && \
    chmod 600 /home/{{USER}}/.ssh/authorized_keys

# Открываем порт
EXPOSE 22

# Команда по умолчанию — запуск SSH‑демона
CMD ["/usr/sbin/sshd", "-D"]
