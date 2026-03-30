//только пример рабочего кода, по факту он не работает сейчас, наверное...

use crate::cloud_lib::CloudContext;

pub struct AppContext {
    cloud: Option<CloudContext>,
    //мы получаем после этого ссылку из .env, и далее идем в облако.
    get: <from ".env"> CLOUD-URL,
    upload: <from .env> CLOUD-UPLOAD-URL, //for example, https://drive.google.com/drive/folders/1GQeqgk8bszdj8dhJfLvwIdmzgHAvgzyW?usp=drive_link
    //загружаются треки в локальную машину и воспроизводятся
    connected: main.rs, //после этого должен начинаться плейбек, но я не знаю, правильно ли я написал.
}

