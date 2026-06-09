use anyhow::{Context, Result, ensure};
use bollard::{
    Docker, body_stream,
    plugin::{ContainerCreateBody, HostConfig, PortBinding, PortMap},
    query_parameters::{
        BuildImageOptionsBuilder, CreateContainerOptions, ListImagesOptions, StopContainerOptions,
    },
};
use futures_util::StreamExt;
use rand::{RngExt, distr::Alphanumeric};
use std::{collections::HashMap, process::Command, time::Duration};
use tokio::{net::TcpStream, time::timeout};
use tracing::debug;
use tracing_test::traced_test;

const DOCKER_IMAGE: &str = "ssh_image";
const CONTAINER_NAME: &str = "ssh_server";
pub const SSH_USER: &str = "testuser";
pub const SSH_PASSWORD: &str = "testpassword";
pub const SSH_KEY_PRIVATE: &str = include_str!("../my_ssh_key");
const SSH_KEY_PUBLIC: &str = include_str!("../my_ssh_key.pub");
const DOCKERFILE_TEMPLATE: &str = include_str!("../Dockerfile");

/// Проверяет, существует ли образ SSH‑сервера
async fn image_exists(docker: &Docker) -> Result<bool> {
    // Создаём фильтр: ищем образы с reference, содержащим SSH_IMAGE
    let mut filters_map = HashMap::new();
    filters_map.insert("reference".to_string(), vec![DOCKER_IMAGE.to_string()]);

    let options = Some(ListImagesOptions {
        all: false, // или true, если нужно включить промежуточные образы
        filters: Some(filters_map),
        ..Default::default()
    });
    // Получаем только подходящие образы
    let images = docker.list_images(options).await?;

    // Если список не пуст — образ существует
    Ok(!images.is_empty())
}

/// Возвращает содержимое Dockerfile для SSH‑сервера с подставленными переменными.
fn dockerfile_content() -> String {
    DOCKERFILE_TEMPLATE
        .replace("{{USER}}", SSH_USER)
        .replace("{{PASSWORD}}", SSH_PASSWORD)
        .replace("{{PUB_KEY}}", SSH_KEY_PUBLIC)
}

/// Создаём Dockerfile для SSH‑сервера и упаковываем его в tar‑архив
///
/// # Returns
///
/// Возвращает содержимое архива в виде байтового вектора.
fn dockerfile_tar() -> Result<Vec<u8>> {
    let dockerfile_contents = dockerfile_content();
    // Создаём tar‑архив с Dockerfile
    let mut tar = tar::Builder::new(Vec::new());
    {
        let mut header = tar::Header::new_gnu();
        header
            .set_path("Dockerfile")
            .context("Не удалось установить путь в tar‑архиве")?;
        header.set_size(dockerfile_contents.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append(&header, dockerfile_contents.as_bytes())
            .context("Не удалось добавить Dockerfile в tar‑архив")?;
    }
    let tar_data = tar
        .into_inner()
        .context("Не удалось завершить создание tar‑архива")?;

    Ok(tar_data)
}

/// Создаёт образ SSH‑сервера с запретом доступа для root
///
/// # Аргументы
/// * `docker` — клиент Docker
async fn create_image(docker: &Docker) -> Result<()> {
    // Получаем путь к уже созданному Dockerfile
    let tar_data = dockerfile_tar()?;
    let payload = Box::new(tar_data).leak();
    let payload = payload.chunks(32);
    let stream = futures_util::stream::iter(payload.map(bytes::Bytes::from));

    // Формируем параметры сборки
    let build_options = BuildImageOptionsBuilder::default()
        .t(DOCKER_IMAGE) // тег образа
        .rm(true)
        .build();

    // Получаем поток событий сборки (без await!)
    let mut image_stream = docker.build_image(build_options, None, Some(body_stream(stream)));

    // Итерируем по потоку событий
    while let Some(result) = image_stream.next().await {
        let info = result.context(
            "Ошибка при сборке Docker‑образа. Проверьте Dockerfile и доступность базового образа",
        )?;

        if let Some(status) = info.status {
            // Фильтруем служебные сообщения
            if status.contains("Downloading")
                || status.contains("Verifying Checksum")
                || status.contains("Pulling from")
                || status.contains("Waiting")
                || status.is_empty()
            {
                continue;
            }
            println!("{}", status);
        }
    }

    println!("✅ Образ '{}' успешно создан", DOCKER_IMAGE);
    Ok(())
}

/// Создаёт и запускает Docker‑контейнер для SSH‑сервера с автоматическим удалением после остановки
///
/// # Arguments
/// * `docker` — клиент Docker (bollard::Docker)
/// * `name` — имя создаваемого контейнера
///
/// # Returns
/// Возвращает `Ok(port)` с номером назначенного порта в случае успеха, `Err` — при ошибке
async fn create_ssh_container(docker: &Docker, name: &str) -> Result<SshServer> {
    // Настраиваем привязки портов: указываем контейнерный порт 22 без привязки к конкретному хостовому порту
    let mut port_bindings = PortMap::new();
    let host_port = PortBinding {
        host_ip: Some("0.0.0.0".to_string()),
        host_port: None, // Автоматическое назначение порта
    };
    port_bindings.insert("22/tcp".to_string(), Some(vec![host_port]));

    // Конфигурируем хост: указываем привязки портов
    let host_config = HostConfig {
        port_bindings: Some(port_bindings),
        ..Default::default()
    };

    // Полная конфигурация контейнера
    let container_create_body = ContainerCreateBody {
        image: Some(DOCKER_IMAGE.to_string()), // Используем фиксированный образ
        host_config: Some(host_config),
        tty: Some(true), // Включаем TTY для интерактивного использования
        ..Default::default()
    };

    // Опции создания контейнера: задаём имя
    let options = CreateContainerOptions {
        name: Some(name.to_string()),
        ..Default::default()
    };

    // Создаём контейнер
    let creation_result = docker
        .create_container(Some(options), container_create_body)
        .await
        .context("Ошибка создания контейнера")?;
    println!("Контейнер создан успешно. ID: {}", creation_result.id);

    docker
        .start_container(name, None)
        .await
        .context("Ошибка запуска контейнера с автоматическим удалением")?;

    // Получаем информацию о контейнере, чтобы узнать назначенный порт
    let container_info = docker
        .inspect_container(&creation_result.id, None)
        .await
        .context("Ошибка получения информации о контейнере")?;

    // Извлекаем назначенный хостовый порт из информации о контейнере
    let port = container_info
        .network_settings
        .and_then(|ns| ns.ports)
        .and_then(|ports| {
            ports
                .get("22/tcp")
                .and_then(|bindings| bindings.as_ref().and_then(|b| b.first()))
                .cloned()
        })
        .and_then(|binding| binding.host_port)
        .and_then(|port| port.parse::<u16>().ok())
        .ok_or_else(|| {
            anyhow::anyhow!("Не удалось получить назначенный порт из настроек сети контейнера")
        })
        .context("Не удалось извлечь назначенный порт")?;

    Ok(SshServer {
        name: name.to_string(),
        port,
    })
}

/// Поднимаем SSH-сервер в Docker-контейнере
///
/// # Returns
/// Возвращает порт, на котором SSH-сервер слушает.
pub async fn up_ssh() -> Result<SshServer> {
    let docker = Docker::connect_with_local_defaults()?;

    // Если контейнер не существует, проверяем есть ли образ SSH-сервера
    if !image_exists(&docker).await? {
        create_image(&docker).await?;
    }

    let suffix = rand::rng()
        .sample_iter(Alphanumeric)
        .take(10)
        .map(char::from)
        .collect::<String>();
    let name = format!("{CONTAINER_NAME}_{}", suffix);
    let server = create_ssh_container(&docker, &name).await?;

    let port = server.port;
    timeout(Duration::from_secs(30), async {
        loop {
            let result = timeout(
                Duration::from_secs(5),
                TcpStream::connect(format!("127.0.0.1:{}", port)),
            )
            .await;
            match result {
                Ok(_) => break,
                Err(_) => continue,
            }
        }
    })
    .await
    .context("SSH-сервер не запустился")?;

    Ok(server)
}

/// Останавливает SSH-сервер затем удаляет его контейнер.
pub async fn stop_ssh(name: &str) -> Result<()> {
    let docker = Docker::connect_with_local_defaults().context("Ошибка подключения к Docker")?;

    debug!("Остановка SSH-сервера {name}");
    // Сначала пытаемся остановить контейнер с таймаутом 5 секунд
    docker
        .stop_container(
            name,
            Some(StopContainerOptions {
                signal: Some("SIGKILL".to_string()),
                t: Some(5),
            }),
        )
        .await
        .with_context(|| format!("Ошибка остановки контейнера {name}"))?;
    debug!("Контейнер {} успешно остановлен", name);

    debug!("Удаление SSH-сервера {name}");
    // Теперь пытаемся удалить контейнер
    docker
        .remove_container(name, None)
        .await
        .map(|_| debug!("Контейнер {name} успешно удален"))
        .with_context(|| format!("Ошибка удаления контейнера {name}"))
}

/// Структура, представляющая SSH-сервер, который автоматически останавливается при уничтожении.
pub struct SshServer {
    name: String,
    port: u16,
}
impl Drop for SshServer {
    fn drop(&mut self) {
        let name = self.name.clone();

        let stop_fn = move || {
            println!("Остановка SSH-сервера {name}");
            // Остановка контейнера с SIGKILL и таймаутом 5 секунд
            let stop_output = Command::new("docker")
                .args(["stop", "--time=5", "--signal=SIGKILL", name.as_str()])
                .output()
                .context(format!("Ошибка выполнения команды docker stop для {name}"))?;

            ensure!(
                stop_output.status.success(),
                "Ошибка остановки контейнера {name}: {}",
                String::from_utf8_lossy(&stop_output.stderr)
            );

            println!("Контейнер {name} успешно остановлен");
            println!("Удаление контейнера {name}");
            // Удаление контейнера
            let rm_output = Command::new("docker")
                .args(["rm", name.as_str()])
                .output()
                .context(format!("Ошибка выполнения команды docker rm для {name}"))?;

            if !rm_output.status.success() {
                return Err(anyhow::anyhow!(
                    "Ошибка удаления контейнера {name}: {}",
                    String::from_utf8_lossy(&rm_output.stderr)
                ));
            }
            debug!("Контейнер {name} успешно остановлен");

            Ok(())
        };

        let name = &self.name;
        match stop_fn() {
            Ok(_) => {
                println!("Контейнер {name} успешно удалён");
            }
            Err(err) => {
                println!("Ошибка остановки контейнера {name}: {err}");
            }
        }
    }
}

#[tokio::test]
#[traced_test]
async fn test_ssh() {
    let _server = up_ssh().await.unwrap();
}
