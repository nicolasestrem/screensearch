//! The invented Spanish-language windows and the observations used to grade them.

use anyhow::{Context, Result};
use chrono::NaiveDate;

use super::{date, Anchor, Beat, DistantLink, Planted, Scene, Storyline, PERSONA_ID};
use crate::gen::layout::Screen;

const FAIR_SITE: &str = "https://feria-turon.example/otono/solicitudes";
const MARKET_SITE: &str = "https://lonja-ribera.example/boletines/tomate-pera";
const WEATHER_SITE: &str = "https://tiempo-ribera.example/llano-del-carrascal";
const CRATE_THREAD: &str = "Ribera Envases | pedido RE-184";
const QUOTE: &str = "Ribera Envases: presupuesto RE-184 por 48 cajas plegables de 10 kg a 6,20 EUR por unidad, transporte incluido.";
const ORDER: &str = "Lucía: confirmo las 48 cajas del pedido RE-184 para la entrega acordada; dejo libre la entrada de la nave.";
const DELAY: &str =
    "Ribera Envases: el camión de RE-184 no salió por avería y las 48 cajas siguen en el almacén.";
const RESCHEDULE: &str =
    "Transporte: RE-184 se ha subido a la ruta de la Ribera; habrá dos bultos de 24 cajas cada uno.";

fn window(app: &'static str, title: &str, body: &[&str]) -> Screen {
    Screen {
        app_hint: app,
        title: title.into(),
        title_bar: true,
        body: body.iter().map(|line| (*line).into()).collect(),
        ..Screen::default()
    }
}

fn chrome(title: &str, url: &str, body: &[&str]) -> Screen {
    let mut screen = window("chrome", &format!("{title} - Google Chrome"), body);
    screen.url = Some(url.into());
    screen
}

fn mail(subject: &str, messages: &[&str]) -> Screen {
    window("OUTLOOK", &format!("{subject} - Outlook"), messages)
}

fn weather(start: NaiveDate, day: u64, forecast: &str, warning: &str) -> Result<Screen> {
    let updated = date(start, day)?.format("%d/%m/%Y");
    Ok(chrome(
        "Pronóstico del Llano del Carrascal",
        WEATHER_SITE,
        &[
            &format!("Boletín meteorológico del {updated}"),
            forecast,
            warning,
        ],
    ))
}

fn scene(day: u64, minute: u32, span_minutes: u32, weight: usize, screen: Screen) -> Scene {
    Scene {
        day,
        minute,
        span_minutes,
        weight,
        primary: true,
        screen,
    }
}

fn quote_board(
    start: NaiveDate,
    day: u64,
    quote: &str,
    comparison: &str,
    other: &str,
) -> Result<Screen> {
    let date = date(start, day)?.format("%d/%m/%Y");
    Ok(chrome(
        "Cotizaciones de tomate pera | Lonja de la Ribera",
        MARKET_SITE,
        &[
            &format!("Boletín de contratación del {date}"),
            quote,
            comparison,
            other,
        ],
    ))
}

pub(super) fn calendar(start: NaiveDate) -> Result<Vec<Scene>> {
    let fair_deadline = date(start, 7)?.format("%d/%m/%Y");
    let fair_edit = date(start, 9)?.format("%d/%m/%Y");
    let crate_eta = date(start, 14)?.format("%d/%m/%Y");
    let crate_revised = date(start, 26)?.format("%d/%m/%Y");
    let mut scenes = vec![
        scene(0, 435, 48, 4, chrome("Convocatoria | Feria de Otoño de Turón", FAIR_SITE, &[
            &format!("La Feria de Otoño de Turón cierra las solicitudes el {fair_deadline} a las 18:00."),
            "Para un puesto de verdura con balanza, solicite el patio C: la toma azul de 6 A es la única con enchufe para ese equipo.",
            "Las plazas libres se adjudican por orden de registro; solo las bajas pasan a lista de reserva.",
        ])),
        scene(1, 455, 43, 4, quote_board(start, 1,
            "Tomate pera de primera categoría: 2,25 EUR/kg en la Lonja de la Ribera, con 310 kg contratados en la primera rueda.",
            "El precio del tomate redondo quedó en 1,95 EUR/kg.",
            "Berenjena rayada: 1,20 EUR/kg; calabacín verde: 0,90 EUR/kg.",
        )?),
        scene(2, 420, 46, 4, window("Acrobat", "Aguaviva-M37-guia-servicio.pdf - Adobe Acrobat Reader", &[
            "Aguaviva M37 | guía de servicio para riego por goteo",
            "Si el cebado pierde presión y gotea por el eje de la Aguaviva M37, compruebe el sello cerámico LQ-37 de 22 mm antes de cambiar el motor.",
            "Con 2 bar sostenidos, deje el retorno abierto cinco minutos para expulsar el aire.",
        ])),
        scene(3, 445, 60, 5, window("EXCEL", "rotacion-parcela-corta.xlsx - Excel", &[
            "Variedad | bancal | bandejas | semillero",
            "El bancal de invierno se reserva para tres variedades: col de hoja Niebla Parda, rábano Cobre Fino y lechuga Bruma de Linde.",
            "Niebla Parda | B2 | 4 bandejas | 96 alveolos",
            "Cobre Fino | B3 | 3 bandejas | 72 alveolos",
            "Bruma de Linde | B4 | 2 bandejas | 48 alveolos",
        ])),
        scene(4, 440, 36, 3, chrome("sello LQ-37 Aguaviva M37 22 mm - Buscar con Google",
            "https://www.google.com/search?q=sello+LQ-37+Aguaviva+M37+22+mm", &[
                "Resultados para sello LQ-37 Aguaviva M37 de 22 mm",
                "Riegos Aljibe lista el sello cerámico LQ-37 para la bomba Aguaviva M37; la ficha excluye el retén de goma de 19 mm.",
                "Almacén del Carrascal: entrega en mostrador si se encarga con referencia de motor.",
            ])),
        scene(5, 480, 35, 3, mail(CRATE_THREAD, &[
            QUOTE,
            &format!("Ribera Envases: si se acepta esta semana, la ruta admite descarga el {crate_eta} por la mañana."),
        ])),
        scene(6, 555, 18, 1, window("WhatsApp", "Riegos Aljibe - WhatsApp", &[
            "Lucía 09:02: La Aguaviva M37 no aguanta el cebado; ¿os queda un sello cerámico LQ-37 de 22 mm?",
            "Nerea 09:06: Quedan dos unidades LQ-37 y puedo guardar una en mostrador a nombre de Hort dels Tarongers.",
        ])),
        scene(7, 560, 4, 0, weather(start, 7,
            "En el Llano del Carrascal se esperan 27 °C y rachas del este de 24 km/h entre las 16:00 y las 18:00.",
            "Lluvia acumulada prevista: 0 mm hasta medianoche.",
        )?),
        scene(8, 460, 53, 4, quote_board(start, 8,
            "El tomate pera de primera cae de 2,25 a 1,85 EUR/kg en la segunda semana de la Lonja de la Ribera; se negociaron 460 kg.",
            "Las existencias de calibre mediano subieron al abrir dos cooperativas la misma ruta.",
            "Pimiento italiano: 1,60 EUR/kg; cebolla seca: 0,65 EUR/kg.",
        )?),
        scene(9, 430, 67, 6, window("WINWORD", "solicitud-feria.docx - Word", &[
            &format!("Feria de Otoño de Turón | borrador de solicitud modificado el {fair_edit}"),
            &format!("El formulario de la Feria de Otoño de Turón cerró el {fair_deadline}; dos días después aún estoy corrigiendo la solicitud para pedir una plaza de reserva."),
            "Adjuntar el seguro de la balanza y el croquis de la toma azul del patio C antes de enviar el correo a secretaría.",
        ])),
        scene(10, 455, 46, 4, mail(CRATE_THREAD, &[
            QUOTE,
            ORDER,
            &format!("Ribera Envases: alta de pedido RE-184 por 297,60 EUR; cargaremos el material el {crate_eta} para primera hora."),
        ])),
        scene(11, 475, 37, 4, mail("Feria de Otoño | solicitud fuera de plazo", &[
            "Registro de Turón: su solicitud llegó fuera de plazo; quedará segunda en lista de reserva para un puesto con corriente.",
            "Secretaría de la feria: mantenemos el croquis de la toma azul de 6 A por si hay una baja en el patio C.",
        ])),
        scene(12, 525, 8, 0, window("EXCEL", "control-invernadero.xlsx - Excel", &[
            "Sonda | zona | lectura a las 08:45",
            "I-01 | pasillo norte | 18,4 °C | 67 % humedad relativa",
            "I-02 | junto a la puerta | 19,1 °C | 61 % humedad relativa",
        ])),
        // Day 13 is intentionally off-screen.
        scene(14, 450, 52, 5, mail(CRATE_THREAD, &[
            QUOTE,
            ORDER,
            DELAY,
            &format!("Lucía: hoy {crate_eta} no ha llegado ninguna caja; guardo las cajas viejas para el mercado del viernes."),
        ])),
        scene(15, 440, 53, 5, quote_board(start, 15,
            "Tercera semana en la Lonja de la Ribera: el tomate pera baja de 1,85 a 1,45 EUR/kg y quedan 130 kg sin comprador.",
            "El lote de primera categoría se ha ofrecido también al puesto de conservas.",
            "Judía plana: 2,10 EUR/kg; acelga: 0,60 EUR por manojo.",
        )?),
        scene(16, 435, 66, 6, window("EXCEL", "plan-semanal-de-venta.xlsx - Excel", &[
            "Producto | entrega viernes | cliente | cantidad prevista",
            "Tomate pera | selección de primera | Mercado de Turón | 140 kg previstos mientras se confirma el precio de la próxima rueda.",
            "Tomate pera | calibre pequeño | venta en finca | 20 kg",
            "Calabacín luna | pieza mediana | Cesta del Carrascal | 30 kg",
            "Cebolla seca | malla de medio kilo | tienda de la plaza | 25 kg",
        ])),
        scene(17, 470, 45, 5, mail("Factura RA-418 | Riegos Aljibe", &[
            "Riegos Aljibe adjunta la factura RA-418 por 34,80 EUR: junta mecánica LQ-37 cerámica de 22 mm, compatible con Aguaviva M37 y ya recogida por Lucía.",
            "Concepto de cargo: una unidad de recambio, sin cambio de motor ni envío.",
        ])),
        scene(18, 520, 58, 4, window("EXCEL", "prueba-cebado-parcela.xlsx - Excel", &[
            "Bomba Aguaviva M37 | medición | presión registrada",
            "Tras montar el sello LQ-37, la Aguaviva M37 mantuvo 2,0 bar durante 25 minutos sin gotear por el eje; el goteo del bancal B vuelve a funcionar.",
            "Arranque | 08:08 | 1,9 bar",
            "Fin de la prueba | 08:33 | 2,0 bar",
        ])),
        scene(19, 530, 35, 3, mail(CRATE_THREAD, &[
            ORDER,
            DELAY,
            RESCHEDULE,
            &format!("Ribera Envases: nueva fecha de descarga para las 48 cajas, {crate_revised} de 09:00 a 12:00."),
        ])),
        scene(20, 615, 5, 0, weather(start, 20,
            "Cielos cubiertos en el Llano del Carrascal, 19 °C al mediodía y 7 mm de lluvia previstos antes de las 14:00.",
            "El aviso por tormentas expira a las 16:30.",
        )?),
        scene(21, 435, 50, 6, quote_board(start, 21,
            "El tomate pera de primera vuelve a bajar en la cuarta semana, esta vez de 1,45 a 1,10 EUR/kg, con salida lenta en la Lonja de la Ribera.",
            "La cooperativa conserva oferta para tomate de salsa si se entrega sin calibrar.",
            "Puerro: 1,05 EUR/kg; pepino corto: 0,75 EUR/kg.",
        )?),
        scene(22, 1280, 23, 3, mail("Pedido ST-603 | Semillas del Umbral", &[
            "Pedido ST-603 para el semillero de invierno: dos sobres de Niebla Parda, uno de Cobre Fino y dos de Bruma de Linde.",
            "Semillas del Umbral: hemos preparado col de hoja, rábano y lechuga; quedan por confirmar los lotes en el albarán.",
        ])),
        scene(23, 460, 44, 5, mail("Puesto C-07 asignado | Feria de Otoño", &[
            "Secretaría de Turón: tras una baja aceptamos su solicitud fuera de plazo y le asignamos el puesto C-07 en el patio, junto al enchufe azul de 6 A para la balanza.",
            "El puesto de verduras de Hort dels Tarongers queda confirmado con montaje a las 06:30 y acceso por la puerta lateral.",
        ])),
        scene(24, 442, 72, 7, window("EXCEL", "plan-semanal-de-venta.xlsx - Excel", &[
            "Producto | salida revisada | destino | cantidad confirmada",
            "Con el tomate pera de la lonja a 1,10 EUR/kg, recorto de 140 a 70 kg para el mercado y envío 50 kg a la cooperativa para salsa.",
            "Tomate pera | cooperativa | retirada sin calibrar | 50 kg",
            "Tomate pera | venta directa | reserva en finca | 20 kg sin cambio",
            "Calabacín luna | Cesta del Carrascal | 30 kg | pendiente de confirmar",
            "Cebolla seca | tienda de la plaza | 25 kg | recogida el viernes",
        ])),
        scene(25, 465, 50, 5, window("Acrobat", "albaran-ST-603.pdf - Adobe Acrobat Reader", &[
            "Semillas del Umbral | albarán ST-603 | entrega en Hort dels Tarongers",
            "Recibidos los cinco sobres del semillero de invierno, con Niebla Parda lote NP-031, Cobre Fino lote CF-174 y Bruma de Linde lote BL-20.",
            "Faltas anotadas en recepción: ninguna; los lotes quedan registrados junto a las bandejas B2, B3 y B4.",
        ])),
        scene(26, 560, 56, 6, mail(CRATE_THREAD, &[
            DELAY,
            RESCHEDULE,
            "Recepción: las 48 cajas plegables de RE-184 llegaron completas en los dos bultos; albarán firmado a las 09:04.",
            "Lucía: separé doce cajas para el reparto y el resto queda limpio en el estante de la nave.",
        ])),
        // The final screen closes the last inactivity gap so day 26 can be staged.
        scene(27, 565, 18, 1, window("WINWORD", "lista-limpieza-despues-del-mercado.docx - Word", &[
            "Lavar las tijeras de cosecha con agua tibia y repasar el mango con un paño seco.",
            "Apuntar dos bobinas de cuerda en la lista del próximo viaje a la cooperativa.",
        ])),
    ];
    for (day, minute, forecast, warning) in [
        (0, 850, "Máxima de 29 °C con viento flojo del sureste durante la tarde.", "La humedad del aire bajará al 39 % a las 17:00."),
        (3, 885, "Nubes bajas al amanecer y máxima de 24 °C al despejar a partir de las 13:30.", "Probabilidad local de lluvia al final de la tarde: 15 %."),
        (5, 840, "Se esperan 11 mm de lluvia en el Llano del Carrascal con una ventana seca entre 15:00 y 16:15.", "Las rachas del norte podrían superar 35 km/h junto a la acequia."),
        (16, 870, "Un chubasco de 3 mm podría cruzar el Llano del Carrascal poco antes de las 18:00.", "Las temperaturas caerán de 23 a 17 °C durante el chubasco."),
        (18, 928, "Cielo despejado con 25 °C a las 16:30 y viento del noreste de 17 km/h.", "El riesgo de precipitación se queda en 10 % por la noche."),
        (23, 895, "Temperatura de 18 °C y llovizna débil de 0,8 mm a las 15:00.", "La previsión da una tregua seca de tres horas desde las 16:15."),
    ] {
        let mut ancillary = scene(day, minute, 12, 1, weather(start, day, forecast, warning)?);
        ancillary.primary = false;
        scenes.push(ancillary);
    }
    for mut ancillary in [
        scene(1, 855, 20, 2, window("EXCEL", "libreta-cobros.xlsx - Excel", &[
            "Cliente | recibo | forma de pago | importe",
            "Panadería Greda | LC-33 | transferencia recibida | 46,80 EUR",
            "Cesta del Carrascal | LC-34 | efectivo pendiente | 28,55 EUR",
        ])),
        scene(8, 861, 18, 2, window("WhatsApp", "Conservas Umbría - WhatsApp", &[
            "Lucía 14:11: El tomate pera ha bajado a 1,85 EUR/kg, ¿os serviría el calibre pequeño para cocer?",
            "Mara 14:16: Esta semana ya tenemos salsa hecha. Pregúntame si baja otra vez antes del próximo turno.",
        ])),
        scene(9, 890, 19, 2, window("explorer", r"C:\Users\Lucia\Documents\Feria - Explorador de archivos", &[
            "solicitud-feria.docx | Documento de Microsoft Word | 29 KB",
            "croquis-balanza-patio-c.pdf | Documento PDF | 76 KB",
        ])),
        scene(11, 874, 14, 1, window("WINWORD", "tarjetas-lotes-de-cebolla.docx - Word", &[
            "Malla pequeña | 500 g | 8 piezas para reponer en la tienda de la plaza",
            "Dejar secar las etiquetas rojas hasta mañana antes de pegar el lote CL-52.",
        ])),
        scene(14, 846, 20, 1, window("EXCEL", "cajas-vacias-nave.xlsx - Excel", &[
            "Fila | caja de repuesto | estado",
            "Estante bajo | 11 de madera | secas y sin clavos a la vista",
            "Pila junto a la puerta | 7 de plástico | enjuagar antes de salir",
        ])),
        scene(17, 910, 16, 1, window("WINWORD", "lista-de-herramientas-cebado.docx - Word", &[
            "Preparar la llave de vaso de 12 mm, un trapo seco y una cubeta limpia para recoger el agua del cebado.",
            "Hacer la prueba con el tramo del bancal B aislado y marcar la hora en la hoja de riego.",
        ])),
        scene(21, 850, 23, 2, window("WhatsApp", "Cooperativa del Pont Sec - WhatsApp", &[
            "Lucía 13:51: La lonja marca 1,10 EUR/kg para el tomate pera. Puedo preparar 50 kg para salsa sin calibrar.",
            "Ona 14:02: Reservo una jaula para esos 50 kg, con recogida al pesar el lote de la mañana.",
        ])),
        scene(24, 871, 18, 1, window("EXCEL", "pesaje-salida-cooperativa.xlsx - Excel", &[
            "Destino | lote | cajas pesadas | total neto",
            "Cooperativa del Pont Sec | TP-18 | 5 cajas | 50,0 kg de tomate pera sin calibrar",
        ])),
        scene(26, 940, 14, 1, window("WhatsApp", "Reparto de la Ribera - WhatsApp", &[
            "Lucía 15:31: Dejo doce cajas limpias junto al portón para cargar los pedidos del lunes.",
            "Hugo 15:36: Paso con la furgoneta a las 06:45 y devuelvo las tres cajas de la ruta anterior.",
        ])),
    ] {
        ancillary.primary = false;
        scenes.push(ancillary);
    }
    Ok(scenes)
}

pub(super) fn planted(start: NaiveDate, scenes: &[Scene]) -> Result<Planted> {
    let anchor = |day: u64, line: usize, detail| -> Result<Anchor> {
        let scene = scenes
            .iter()
            .find(|scene| scene.primary && scene.day == day)
            .with_context(|| format!("missing planted primary window on day {day}"))?;
        Ok(Anchor {
            date: date(start, day)?.to_string(),
            app: scene.screen.app_hint,
            window: scene.screen.title.clone(),
            detail,
            evidence: scene
                .screen
                .body
                .get(line)
                .with_context(|| format!("missing planted line {line} on day {day}"))?
                .clone(),
        })
    };
    let beat = |stage, day, line, detail| -> Result<Beat> {
        Ok(Beat {
            stage,
            window: anchor(day, line, detail)?,
        })
    };
    Ok(Planted {
        persona: PERSONA_ID,
        start: start.to_string(),
        storylines: vec![
            Storyline {
                id: "late-fair-application",
                title: "Una solicitud fuera de plazo consigue puesto",
                beats: vec![
                    beat("start", 0, 0, "La convocatoria de la Feria de Otoño de Turón fija un plazo.")?,
                    beat("development", 9, 1, "Lucía sigue editando la solicitud dos días después del cierre.")?,
                    beat("development", 11, 0, "Secretaría registra la solicitud tardía en lista de reserva.")?,
                    beat("outcome", 23, 0, "Una baja en la feria permite asignar el puesto C-07.")?,
                ],
            },
            Storyline {
                id: "falling-tomato-price",
                title: "La caída del tomate pera cambia el plan de venta",
                beats: vec![
                    beat("start", 1, 1, "La primera cotización del tomate pera es 2,25 EUR/kg.")?,
                    beat("development", 8, 1, "En la segunda semana el precio baja a 1,85 EUR/kg.")?,
                    beat("development", 15, 1, "En la tercera semana el precio baja a 1,45 EUR/kg.")?,
                    beat("development", 21, 1, "En la cuarta semana el precio baja a 1,10 EUR/kg.")?,
                    beat("outcome", 24, 1, "Reduce de 140 a 70 kg el lote para el mercado y desvía 50 kg a la cooperativa.")?,
                ],
            },
            Storyline {
                id: "winter-seed-order",
                title: "El plan del semillero se convierte en pedido y entrega",
                beats: vec![
                    beat("start", 3, 1, "La rotación reserva tres variedades distintas en tres bancales.")?,
                    beat("development", 22, 0, "El pedido ST-603 solicita las mismas variedades de invierno.")?,
                    beat("outcome", 25, 1, "El albarán registra la llegada de cinco sobres y sus lotes.")?,
                ],
            },
            Storyline {
                id: "pump-seal-breakdown",
                title: "La bomba Aguaviva M37 vuelve a cebar",
                beats: vec![
                    beat("start", 2, 1, "El manual de la bomba atribuye la pérdida de cebado al sello LQ-37 de 22 mm.")?,
                    beat("development", 4, 1, "La búsqueda distingue el sello correcto del retén de 19 mm.")?,
                    beat("development", 6, 0, "Lucía pide el sello por WhatsApp a Riegos Aljibe.")?,
                    beat("development", 17, 0, "La factura RA-418 confirma la recogida del sello por 34,80 EUR.")?,
                    beat("outcome", 18, 1, "La prueba tras el montaje mantiene 2 bar durante 25 minutos.")?,
                ],
            },
            Storyline {
                id: "delayed-crate-order",
                title: "Las cajas RE-184 llegan tras una avería",
                beats: vec![
                    beat("start", 5, 0, "El presupuesto RE-184 oferta 48 cajas a 6,20 EUR por unidad.")?,
                    beat("development", 10, 1, "Lucía confirma 48 cajas para la entrega acordada.")?,
                    beat("development", 14, 2, "La avería del camión impide cumplir la entrega.")?,
                    beat("development", 19, 3, "Ribera Envases fija una nueva fecha de descarga.")?,
                    beat("outcome", 26, 2, "Llegan las 48 cajas y se firma el albarán.")?,
                ],
            },
        ],
        distant_links: vec![
            DistantLink {
                id: "same-three-seed-varieties",
                first: anchor(3, 1, "Plan de rotación en Excel con tres variedades de invierno.")?,
                second: anchor(22, 0, "Solicitud de cinco sobres en Outlook, 19 días después.")?,
                connection: "El pedido de semillas vuelve a reunir Niebla Parda, Cobre Fino y Bruma de Linde.",
            },
            DistantLink {
                id: "same-pump-replacement-part",
                first: anchor(2, 1, "La guía en Acrobat identifica el sello cerámico LQ-37 de 22 mm.")?,
                second: anchor(17, 0, "Outlook muestra la factura de la junta mecánica, 15 días después.")?,
                connection: "El recambio que se recoge es el LQ-37 de 22 mm de la Aguaviva M37 del manual.",
            },
            DistantLink {
                id: "same-powered-fair-stall",
                first: anchor(0, 1, "La convocatoria en Chrome indica un lugar con alimentación para la balanza.")?,
                second: anchor(23, 0, "Outlook confirma una plaza de reserva, 23 días después.")?,
                connection: "El puesto asignado usa la toma azul de 6 A del patio C que indicaba la convocatoria.",
            },
        ],
    })
}
